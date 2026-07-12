use std::path::{Path};
use std::sync::Arc;

use crate::engine::dict_data::DictDataReader;
use crate::engine::ifo_parser::parse_ifo;
use crate::engine::idx_parser::{self, binary_search_all, build_sorted_index, prefix_scan};
use crate::engine::syn_parser::SynonymIndex;
use crate::engine::types::{ArticleData, IfoInfo, IndexEntry, SearchResult};

/// 单个词典的完整读取器

pub struct DictionaryReader {
    /// 词典元信息
    pub info: IfoInfo,
    /// 索引条目（原始顺序）
    entries: Vec<IndexEntry>,
    /// 排序索引（按 word 排序的 entries 偏移）
    sorted: Vec<usize>,
    /// 数据文件读取器（共享引用）
    dict_data: Arc<DictDataReader>,
    /// 同义词索引（可选）
    synonym_index: Option<SynonymIndex>,
}

impl DictionaryReader {
    /// 从 .ifo 路径打开词典
    pub fn open(ifo_path: &Path) -> Result<Self, String> {
        let ifo_content =
            std::fs::read_to_string(ifo_path).map_err(|e| format!("Read ifo error: {}", e))?;
        let info = parse_ifo(&ifo_content)?;

        // 加载索引
        let idx_path = ifo_path.with_extension("idx");
        let idx_path_gz = ifo_path.with_extension("idx.gz");

        let idx_path = if idx_path_gz.exists() {
            idx_path_gz
        } else if idx_path.exists() {
            idx_path
        } else {
            return Err(format!("No .idx or .idx.gz found for {}", ifo_path.display()));
        };

        let idxoffsetbits = info.idxoffsetbits.unwrap_or(32);
        let entries = idx_parser::load_index_from_file(&idx_path, idxoffsetbits)?;

        // 构建排序索引
        let sorted = build_sorted_index(&entries);

        // 打开数据文件
        let dict_path = ifo_path.with_extension(""); // 去掉 .ifo 后缀
        let dict_data = Arc::new(DictDataReader::open(&dict_path)?);

        // 加载同义词（如果存在）
        let synonym_index = {
            let syn_path = ifo_path.with_extension("syn");
            if syn_path.exists() {
                let syn_data = std::fs::read(&syn_path).map_err(|e| format!("Read syn: {}", e))?;
                let original_words: Vec<String> = entries.iter().map(|e| e.word.clone()).collect();
                Some(SynonymIndex::parse(&syn_data, &original_words))
            } else {
                None
            }
        };

        log::info!(
            "Loaded dictionary '{}': {} words, {} synonyms",
            info.bookname,
            entries.len(),
            synonym_index.as_ref().map(|s| s.len()).unwrap_or(0)
        );

        Ok(DictionaryReader {
            info,
            entries,
            sorted,
            dict_data,
            synonym_index,
        })
    }

    /// 精确查找一个词（返回所有释义条目合并），大小写不敏感
    pub fn lookup_exact(&self, word: &str) -> Option<ArticleData> {
        // 精确匹配
        let indices = binary_search_all(&self.entries, &self.sorted, word);
        if !indices.is_empty() {
            return Some(self.read_articles_combined(&indices));
        }

        // 大小写不敏感回退：全小写精确匹配
        let word_lower = word.to_lowercase();
        if word_lower != word {
            let indices = binary_search_all(&self.entries, &self.sorted, &word_lower);
            if !indices.is_empty() {
                return Some(self.read_articles_combined(&indices));
            }
        }

        // 大小写不敏感回退：线性扫描
        let indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.word.to_lowercase() == word_lower)
            .map(|(i, _)| i)
            .collect();
        if !indices.is_empty() {
            return Some(self.read_articles_combined(&indices));
        }

        // 尝试同义词
        if let Some(ref syn) = self.synonym_index {
            if let Some(orig_word) = syn.lookup(word) {
                let indices = binary_search_all(&self.entries, &self.sorted, orig_word);
                if !indices.is_empty() {
                    return Some(self.read_articles_combined(&indices));
                }
            }
        }

        None
    }

    /// 合并多个索引条目的释义为单个 ArticleData
    fn read_articles_combined(&self, indices: &[usize]) -> ArticleData {
        let mut combined_text = String::new();
        let is_html = self
            .info
            .sametypesequence
            .as_ref()
            .map(|s| s.contains('h') || s.contains('H'))
            .unwrap_or(false);

        let multi_entry = indices.len() > 1;

        for (i, &idx) in indices.iter().enumerate() {
            if let Ok(article) = self.read_article(idx) {
                if i > 0 {
                    combined_text.push_str("\n\n");
                }
                if multi_entry {
                    combined_text.push_str(&format!("【释义 {}】\n", i + 1));
                }
                combined_text.push_str(&article.raw_text);
            }
        }

        ArticleData {
            raw_text: combined_text,
            is_html,
            dict_name: self.info.bookname.clone(),
        }
    }

    /// 前缀扫描
    pub fn lookup_prefix(&self, prefix: &str, limit: usize) -> Vec<SearchResult> {
        let indices = prefix_scan(&self.entries, &self.sorted, prefix, limit);
        indices
            .into_iter()
            .map(|idx| SearchResult {
                dict_name: self.info.bookname.clone(),
                word: self.entries[idx].word.clone(),
                score: 0.7,
            })
            .collect()
    }

    /// 模糊匹配（Levenshtein）
    pub fn lookup_fuzzy(
        &self,
        word: &str,
        threshold: usize,
        limit: usize,
    ) -> Vec<SearchResult> {
        use crate::engine::fuzzy_matcher::{levenshtein_distance, similarity};

        let mut results: Vec<(usize, f64)> = Vec::new();
        for &i in &self.sorted {
            // Skip entries too long (fuzzy matching long words is noisy)
            if self.entries[i].word.len() > word.len() + threshold + 3 {
                continue;
            }
            let dist = levenshtein_distance(word, &self.entries[i].word, threshold);
            if dist != usize::MAX {
                let sim = similarity(word, &self.entries[i].word);
                results.push((i, sim));
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        results
            .into_iter()
            .map(|(idx, score)| SearchResult {
                dict_name: self.info.bookname.clone(),
                word: self.entries[idx].word.clone(),
                score: score as f32 * 0.5,
            })
            .collect()
    }

    /// 读取指定索引的文章数据
    pub fn read_article(&self, idx: usize) -> Result<ArticleData, String> {
        let entry = &self.entries[idx];
        self.read_article_data(entry)
    }

    /// 读取 IndexEntry 对应的文章
    pub fn read_article_data(&self, entry: &IndexEntry) -> Result<ArticleData, String> {
        let bytes = self.dict_data.read_at(entry.data_offset, entry.data_size)?;
        let raw_text = String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {}", e))?;

        let is_html = self
            .info
            .sametypesequence
            .as_ref()
            .map(|s| s.contains('h') || s.contains('H'))
            .unwrap_or(false);

        Ok(ArticleData {
            raw_text,
            is_html,
            dict_name: self.info.bookname.clone(),
        })
    }

    /// 词条总数
    pub fn word_count(&self) -> usize {
        self.entries.len()
    }

    
}
