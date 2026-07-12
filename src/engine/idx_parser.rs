use crate::engine::types::IndexEntry;
use std::io::Read;

/// 加载 .idx 或 .idx.gz 文件为 IndexEntry 向量
pub fn load_index(data: &[u8], idxoffsetbits: u8) -> Vec<IndexEntry> {
    let mut entries = Vec::new();
    let mut pos = 0usize;
    let len = data.len();

    while pos < len {
        // 1. 读取以 \0 结尾的词头（UTF-8）
        let word_end = match data[pos..].iter().position(|&b| b == 0) {
            Some(end) => end,
            None => break, // 文件结尾可能没有完整条目
        };

        let word = match std::str::from_utf8(&data[pos..pos + word_end]) {
            Ok(s) => s.to_string(),
            Err(_) => {
                // 跳过无效 UTF-8
                pos += word_end + 1;
                continue;
            }
        };
        pos += word_end + 1; // 跳过 \0

        // 确保有足够数据
        let entry_size = if idxoffsetbits == 64 { 16 } else { 8 };
        if pos + entry_size > len {
            break;
        }

        // 2. 读取 offset 和 size（大端序）
        if idxoffsetbits == 64 {
            let offset = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
            pos += 8;
            let size = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
            pos += 8;
            entries.push(IndexEntry {
                word,
                data_offset: offset,
                data_size: size,
            });
        } else {
            let offset = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as u64;
            pos += 4;
            let size = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as u64;
            pos += 4;
            entries.push(IndexEntry {
                word,
                data_offset: offset,
                data_size: size,
            });
        }
    }

    entries
}

/// 从原始数据加载索引（自动处理 .gz 解压）
pub fn load_index_from_file(path: &std::path::Path, idxoffsetbits: u8) -> Result<Vec<IndexEntry>, String> {
    let raw = std::fs::read(path).map_err(|e| format!("Failed to read idx file: {}", e))?;

    // 检查是否为 gzip 压缩
    if raw.len() >= 2 && raw[0] == 0x1f && raw[1] == 0x8b {
        // .idx.gz
        let mut decoder = flate2::read::GzDecoder::new(&raw[..]);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .map_err(|e| format!("Failed to decompress idx.gz: {}", e))?;
        Ok(load_index(&decompressed, idxoffsetbits))
    } else {
        // .idx (未压缩)
        Ok(load_index(&raw, idxoffsetbits))
    }
}

/// 为 IndexEntry 向量建立排序索引（按 word 排序的偏移数组）
pub fn build_sorted_index(entries: &[IndexEntry]) -> Vec<usize> {
    let mut sorted: Vec<usize> = (0..entries.len()).collect();
    sorted.sort_by_key(|&i| entries[i].word.as_str());
    sorted
}

/// 二分查找所有精确匹配（同一词头可能有多个释义条目）
pub fn binary_search_all(entries: &[IndexEntry], sorted: &[usize], word: &str) -> Vec<usize> {
    let pos = match sorted.binary_search_by(|&i| entries[i].word.as_str().cmp(word)) {
        Ok(pos) => pos,
        Err(_) => return Vec::new(),
    };
    let mut results = vec![sorted[pos]];
    // 向前扫描相同词条
    let mut p = pos as isize - 1;
    while p >= 0 {
        let idx = p as usize;
        if entries[sorted[idx]].word == word {
            results.push(sorted[idx]);
            p -= 1;
        } else {
            break;
        }
    }
    // 向后扫描相同词条
    p = pos as isize + 1;
    while p < sorted.len() as isize {
        let idx = p as usize;
        if entries[sorted[idx]].word == word {
            results.push(sorted[idx]);
            p += 1;
        } else {
            break;
        }
    }
    results
}

/// 前缀扫描：返回所有以 prefix 开头的条目索引
pub fn prefix_scan(
    entries: &[IndexEntry],
    sorted: &[usize],
    prefix: &str,
    limit: usize,
) -> Vec<usize> {
    // 用二分查找定位前缀起始位置
    let start = sorted.partition_point(|&i| entries[i].word.as_str() < prefix);
    let mut results = Vec::with_capacity(limit);

    for &i in &sorted[start..] {
        if !entries[i].word.starts_with(prefix) {
            break;
        }
        results.push(i);
        if results.len() >= limit {
            break;
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_idx_data(entries: &[(&str, u32, u32)]) -> Vec<u8> {
        let mut data = Vec::new();
        for (word, offset, size) in entries {
            data.extend_from_slice(word.as_bytes());
            data.push(0);
            data.extend_from_slice(&offset.to_be_bytes());
            data.extend_from_slice(&size.to_be_bytes());
        }
        data
    }

    #[test]
    fn test_load_index_basic() {
        let data = make_idx_data(&[
            ("apple", 0, 100),
            ("banana", 100, 200),
            ("cherry", 300, 150),
        ]);
        let entries = load_index(&data, 32);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].word, "apple");
        assert_eq!(entries[0].data_offset, 0);
        assert_eq!(entries[0].data_size, 100);
        assert_eq!(entries[2].word, "cherry");
    }

    #[test]
    fn test_binary_search() {
        let data = make_idx_data(&[
            ("apple", 0, 100),
            ("banana", 100, 200),
            ("cherry", 300, 150),
        ]);
        let entries = load_index(&data, 32);
        let sorted = build_sorted_index(&entries);

        assert_eq!(binary_search(&entries, &sorted, "banana"), Some(1));
        assert_eq!(binary_search(&entries, &sorted, "zebra"), None);
    }

    #[test]
    fn test_prefix_scan() {
        let data = make_idx_data(&[
            ("apple", 0, 100),
            ("application", 100, 200),
            ("apricot", 300, 150),
            ("banana", 450, 100),
        ]);
        let entries = load_index(&data, 32);
        let sorted = build_sorted_index(&entries);

        let results = prefix_scan(&entries, &sorted, "ap", 10);
        assert_eq!(results.len(), 3);
    }
}
