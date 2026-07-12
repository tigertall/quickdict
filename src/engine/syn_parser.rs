use std::collections::HashMap;

/// 同义词索引：synonym -> original_word 的映射
#[derive(Debug, Clone)]
pub struct SynonymIndex {
    map: HashMap<String, String>,
}

impl SynonymIndex {
    /// 解析 .syn 文件
    ///
    /// .syn 格式与 .idx 相同: `synonym_word\0 + original_word_index:u32(BE)`
    /// original_word_index 指向 .idx 中的词条索引
    pub fn parse(data: &[u8], original_words: &[String]) -> Self {
        let mut map = HashMap::new();
        let mut pos = 0;
        let len = data.len();

        while pos < len {
            let word_end = match data[pos..].iter().position(|&b| b == 0) {
                Some(end) => end,
                None => break,
            };

            let synonym = match std::str::from_utf8(&data[pos..pos + word_end]) {
                Ok(s) => s.to_string(),
                Err(_) => {
                    pos += word_end + 1 + 4;
                    continue;
                }
            };
            pos += word_end + 1;

            if pos + 4 > len {
                break;
            }

            let orig_idx = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;

            if orig_idx < original_words.len() {
                map.insert(synonym, original_words[orig_idx].clone());
            }
        }

        SynonymIndex { map }
    }

    /// 查找同义词对应的原始词
    pub fn lookup(&self, synonym: &str) -> Option<&str> {
        self.map.get(synonym).map(|s| s.as_str())
    }

    /// 同义词数量
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_syn() {
        let mut data = Vec::new();
        // synonym "colour" -> index 0 ("color")
        data.extend_from_slice(b"colour");
        data.push(0);
        data.extend_from_slice(&0u32.to_be_bytes());

        // synonym "center" -> index 1 ("centre")
        data.extend_from_slice(b"center");
        data.push(0);
        data.extend_from_slice(&1u32.to_be_bytes());

        let original_words = vec!["color".to_string(), "centre".to_string()];
        let syn_idx = SynonymIndex::parse(&data, &original_words);

        assert_eq!(syn_idx.lookup("colour"), Some("color"));
        assert_eq!(syn_idx.lookup("center"), Some("centre"));
        assert_eq!(syn_idx.lookup("unknown"), None);
    }
}
