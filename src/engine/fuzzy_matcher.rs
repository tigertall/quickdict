use unicode_normalization::UnicodeNormalization;

/// Levenshtein 编辑距离算法（单行优化版本，带提前剪枝）
///
/// 返回两个字符串之间的编辑距离。
/// 如果距离超过 threshold，返回 usize::MAX 以表示不匹配。
pub fn levenshtein_distance(a: &str, b: &str, threshold: usize) -> usize {
    let a_chars: Vec<char> = a.nfd().collect();
    let b_chars: Vec<char> = b.nfd().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    // 优化：如果长度差超过阈值，直接返回 MAX
    if m.abs_diff(n) > threshold {
        return usize::MAX;
    }

    // 优化：只保留两行
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        let mut min_in_row = curr[0];

        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1) // 删除
                .min(curr[j - 1] + 1) // 插入
                .min(prev[j - 1] + cost); // 替换
            min_in_row = min_in_row.min(curr[j]);
        }

        // 提前剪枝：如果整行最小值超过阈值，直接返回
        if min_in_row > threshold {
            return usize::MAX;
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    if prev[n] > threshold {
        usize::MAX
    } else {
        prev[n]
    }
}

/// 计算两个字符串的相似度分数（0.0 ~ 1.0）
pub fn similarity(a: &str, b: &str) -> f64 {
    let max_len = a.chars().count().max(b.chars().count()) as f64;
    if max_len == 0.0 {
        return 1.0;
    }
    let dist = levenshtein_distance(a, b, max_len as usize);
    if dist == usize::MAX {
        0.0
    } else {
        1.0 - (dist as f64 / max_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_basic() {
        assert_eq!(levenshtein_distance("kitten", "sitting", 10), 3);
        assert_eq!(levenshtein_distance("hello", "hello", 10), 0);
        assert_eq!(levenshtein_distance("abc", "def", 10), 3);
    }

    #[test]
    fn test_levenshtein_threshold() {
        assert_eq!(levenshtein_distance("hello", "world", 2), usize::MAX);
        assert_eq!(levenshtein_distance("hello", "world", 5), 4);
    }

    #[test]
    fn test_similarity() {
        assert!(similarity("hello", "hello") > 0.99);
        assert!(similarity("hello", "hallo") > 0.7);
        assert_eq!(similarity("abc", "xyz"), 0.0);
    }

    #[test]
    fn test_unicode_norm() {
        // NFC vs NFD normalization
        let d1 = levenshtein_distance("café", "cafe\u{0301}", 10);
        assert_eq!(d1, 0); // should match after normalization
    }
}
