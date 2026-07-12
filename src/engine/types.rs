use serde::{Deserialize, Serialize};

/// .idx 中的单条索引条目
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub word: String,
    pub data_offset: u64,
    pub data_size: u64,
}

/// 词典元信息（.ifo）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfoInfo {
    pub version: String,
    pub bookname: String,
    pub wordcount: u64,
    pub synwordcount: Option<u64>,
    pub idxfilesize: u64,
    pub idxoffsetbits: Option<u8>,
    pub author: Option<String>,
    pub email: Option<String>,
    pub website: Option<String>,
    pub description: Option<String>,
    pub date: Option<String>,
    pub sametypesequence: Option<String>,
    pub dicttype: Option<String>,
}

impl Default for IfoInfo {
    fn default() -> Self {
        Self {
            version: String::new(), bookname: String::new(), wordcount: 0,
            synwordcount: None, idxfilesize: 0, idxoffsetbits: Some(32),
            author: None, email: None, website: None, description: None,
            date: None, sametypesequence: None, dicttype: None,
        }
    }
}


/// 单个搜索结果
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub dict_name: String,
    pub word: String,
    pub score: f32,
}

/// 词典条目数据
#[derive(Debug, Clone)]
pub struct ArticleData {
    pub raw_text: String,
    pub is_html: bool,
    pub dict_name: String,
}

/// 词典类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DictKind {
    StarDict,
    Mdx,
    Online(String),  // 在线词典标识（如 "baidu"）
}

impl DictKind {
    pub fn is_online(&self) -> bool { matches!(self, DictKind::Online(_)) }
    pub fn online_id(&self) -> Option<&str> {
        match self { DictKind::Online(id) => Some(id.as_str()), _ => None }
    }
}

/// 词典信息（用于UI显示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictInfo {
    pub name: String,
    pub path: String,
    pub word_count: u64,
    pub author: Option<String>,
    pub description: Option<String>,
    pub enabled: bool,
    pub kind: DictKind,
}

/// 统一的词典查询接口

pub trait Dictionary: Send + Sync {
    fn name(&self) -> &str;
    fn word_count(&self) -> usize;
    fn kind(&self) -> DictKind;
    fn lookup_exact(&self, word: &str) -> Option<ArticleData>;
    fn as_any(&self) -> &dyn std::any::Any;
}
