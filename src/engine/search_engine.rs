use std::collections::HashSet;

use crate::engine::dict_manager::DictManager;
use crate::engine::types::SearchResult;

/// Strip leading/trailing punctuation from a query word
pub(crate) fn clean_word(word: &str) -> String {
    let trimmed = word.trim();
    if trimmed.is_empty() { return String::new(); }
    let chars: Vec<char> = trimmed.chars().collect();
    let mut start = 0;
    let mut end = chars.len();
    // Strip leading non-alphanumeric chars (but keep leading quotes for contractions like 'tis)
    while start < end && is_strippable(chars[start]) {
        start += 1;
    }
    // Strip trailing non-alphanumeric chars
    while end > start && is_strippable(chars[end - 1]) {
        end -= 1;
    }
    if start >= end { return trimmed.to_string(); } // Keep original if all stripped
    let result: String = chars[start..end].iter().collect();
    result
}

fn is_strippable(c: char) -> bool {
    !c.is_alphanumeric() && c != '\'' && c != '-' && c != '’'
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub prefix_min_len: usize,
    pub prefix_limit: usize,
    pub fuzzy_threshold: usize,
    pub fuzzy_limit: usize,
    pub max_results: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self { prefix_min_len: 2, prefix_limit: 20, fuzzy_threshold: 3, fuzzy_limit: 10, max_results: 50 }
    }
}

#[derive(Debug, Clone)]
pub struct SearchEngine { config: SearchConfig }

impl SearchEngine {
    pub fn new(config: SearchConfig) -> Self { Self { config } }

    pub fn search(&self, query: &str, manager: &DictManager) -> Vec<SearchResult> {
        let query = clean_word(query);
        if query.is_empty() || query.len() > 30 { return Vec::new(); }
        let mut results: Vec<SearchResult> = Vec::new();

        for dict in manager.enabled() {
            if let Some(article) = dict.lookup_exact(&query) {
                results.push(SearchResult {
                    dict_name: article.dict_name.clone(),
                    word: query.to_string(),
                    score: 1.0,
                });
            }
        }
        if !results.is_empty() {
            return Self::dedup(results);
        }

        if query.len() >= self.config.prefix_min_len {
            for dict in manager.enabled() {
                results.extend(dict.lookup_prefix(&query, self.config.prefix_limit));
            }
        }
        if !results.is_empty() {
            results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            results.truncate(self.config.max_results);
            return Self::dedup(results);
        }

        for dict in manager.enabled() {
            results.extend(
                dict.lookup_fuzzy(&query, self.config.fuzzy_threshold, self.config.fuzzy_limit),
            );
        }
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(self.config.max_results);
        Self::dedup(results)
    }

    fn dedup(results: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut seen: HashSet<String> = HashSet::new();
        results.into_iter().filter(|r| seen.insert(format!("{}|{}", r.dict_name, r.word))).collect()
    }
}
