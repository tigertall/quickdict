use std::path::PathBuf;
use std::sync::Arc;

use crate::engine::dict_reader::DictionaryReader;
use crate::engine::mdx_reader::MdxReader;
use crate::engine::types::{ArticleData, DictInfo, DictKind, Dictionary, SearchResult};

/// 多词典管理器
pub struct DictManager {
    dicts: Vec<Arc<dyn Dictionary>>,
    active: Vec<bool>,
    mdict_paths_index: Vec<Option<String>>, // path for each dict entry
}

impl DictManager {
    pub fn new() -> Self {
        Self {
            dicts: Vec::new(),
            active: Vec::new(),
            mdict_paths_index: Vec::new(),
        }
    }

    // ========== 统一接口 ==========

    pub fn all(&self) -> &[Arc<dyn Dictionary>] {
        &self.dicts
    }

    pub fn is_active(&self, idx: usize) -> bool {
        self.active.get(idx).copied().unwrap_or(false)
    }

    /// 所有启用的词典（trait对象）
    pub fn enabled(&self) -> Vec<&dyn Dictionary> {
        self.dicts
            .iter()
            .enumerate()
            .filter(|(i, _)| self.active.get(*i).copied().unwrap_or(false))
            .map(|(_, d)| d.as_ref())
            .collect()
    }

    pub fn toggle_dict(&mut self, id: usize, enabled: bool) {
        if id < self.active.len() {
            self.active[id] = enabled;
        }
    }

    pub fn dict_infos(&self) -> Vec<DictInfo> {
        self.dicts
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.kind().is_online()) // exclude online dicts (shown separately)
            .map(|(i, d)| DictInfo {
                name: d.name().to_string(),
                path: self
                    .mdict_paths_index
                    .get(i)
                    .and_then(|p| p.clone())
                    .unwrap_or_default(),
                word_count: d.word_count() as u64,
                author: None,
                description: None,
                enabled: self.active.get(i).copied().unwrap_or(true),
                kind: d.kind(),
            })
            .collect()
    }

    /// 查询所有已启用词典

    /// 查询本地词典（不含在线）
    pub fn lookup_local(&self, word: &str) -> Vec<ArticleData> {
        self.enabled()
            .iter()
            .filter(|d| !d.kind().is_online())
            .filter_map(|d| d.lookup_exact(word))
            .collect()
    }

    /// 查询在线词典，返回第一个命中
    pub fn try_online(&self, word: &str) -> Option<ArticleData> {
        for d in self.enabled() {
            if d.kind().is_online() {
                if let Some(a) = d.lookup_exact(word) {
                    return Some(a);
                }
            }
        }
        None
    }

    // ========== StarDict 兼容 ==========

    /// 仅启用的 StarDict（返回原始 Reader 供 search_engine 使用）
    pub fn active_stardict(&self) -> Vec<&DictionaryReader> {
        self.dicts
            .iter()
            .enumerate()
            .filter(|(i, _)| self.active.get(*i).copied().unwrap_or(false))
            .filter(|(_, d)| d.kind() == DictKind::StarDict)
            .filter_map(|(_, d)| {
                d.as_any()
                    .downcast_ref::<StarDictDict>()
                    .map(|sd| sd.inner.as_ref())
            })
            .collect()
    }

    pub fn active_dict_names(&self) -> Vec<String> {
        self.active_stardict()
            .iter()
            .map(|d| d.info.bookname.clone())
            .collect()
    }

    /// 清空所有词典（保留在线词典需重新添加）
    pub fn clear_all(&mut self) {
        self.dicts.clear();
        self.active.clear();
        self.mdict_paths_index.clear();
    }

    // ========== 排序 ==========

    /// 交换两个词典位置
    pub fn swap_dicts(&mut self, i: usize, j: usize) {
        if i >= self.dicts.len() || j >= self.dicts.len() {
            return;
        }
        self.dicts.swap(i, j);
        self.active.swap(i, j);
        self.mdict_paths_index.swap(i, j);
    }

    /// 上移词典
    pub fn move_dict_up(&mut self, idx: usize) {
        if idx > 0 {
            self.swap_dicts(idx, idx - 1);
        }
    }

    /// 下移词典
    pub fn move_dict_down(&mut self, idx: usize) {
        if idx + 1 < self.dicts.len() {
            self.swap_dicts(idx, idx + 1);
        }
    }

    /// 导出当前词典顺序（用于持久化）
    pub fn export_order(&self) -> Vec<String> {
        self.dicts
            .iter()
            .map(|d| match d.kind() {
                DictKind::Online(ref id) => format!("Online:{}", id),
                DictKind::Mdx => format!("Mdx:{}", d.name()),
                DictKind::StarDict => format!("StarDict:{}", d.name()),
            })
            .collect()
    }

    /// 按照保存的顺序重新排列词典
    pub fn reorder_by(&mut self, order: &[String]) {
        if order.is_empty() {
            return;
        }
        let mut position: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (i, key) in order.iter().enumerate() {
            position.insert(key.clone(), i);
        }
        let mut indexed: Vec<(usize, usize)> = self
            .dicts
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let key = match d.kind() {
                    DictKind::Online(ref id) => format!("Online:{}", id),
                    DictKind::Mdx => format!("Mdx:{}", d.name()),
                    DictKind::StarDict => format!("StarDict:{}", d.name()),
                };
                (i, position.get(&key).copied().unwrap_or(usize::MAX))
            })
            .collect();
        indexed.sort_by_key(|&(_, pos)| pos);
        let old_dicts = std::mem::take(&mut self.dicts);
        let old_active = std::mem::take(&mut self.active);
        let old_paths = std::mem::take(&mut self.mdict_paths_index);
        for (old_idx, _) in &indexed {
            self.dicts.push(old_dicts[*old_idx].clone());
            self.active.push(old_active[*old_idx]);
            self.mdict_paths_index.push(old_paths[*old_idx].clone());
        }
    }

    // ========== 在线 ==========

    pub fn add_online_dict(&mut self, dict: Arc<dyn Dictionary>) {
        if let Some(id) = dict.kind().online_id() {
            if self.online_idx(id).is_some() {
                return; // Already registered
            }
        }
        self.dicts.push(dict);
        self.active.push(true);
        self.mdict_paths_index.push(None);
    }

    /// Find the index of the online dict with given service_id (e.g. "baidu")
    pub fn online_idx(&self, service_id: &str) -> Option<usize> {
        self.dicts
            .iter()
            .position(|d| d.kind().online_id() == Some(service_id))
    }

    /// Export all dict active states as (kind:name, enabled) pairs
    pub fn export_active_states(&self) -> Vec<(String, bool)> {
        self.dicts
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let key = match d.kind() {
                    DictKind::Online(ref id) => format!("Online:{}", id),
                    DictKind::Mdx => format!("Mdx:{}", d.name()),
                    DictKind::StarDict => format!("StarDict:{}", d.name()),
                };
                (key, self.active.get(i).copied().unwrap_or(true))
            })
            .collect()
    }

    /// Restore active states from exported data
    pub fn import_active_states(&mut self, states: &[(String, bool)]) {
        for (i, d) in self.dicts.iter().enumerate() {
            let key = match d.kind() {
                DictKind::Online(ref id) => format!("Online:{}", id),
                DictKind::Mdx => format!("Mdx:{}", d.name()),
                DictKind::StarDict => format!("StarDict:{}", d.name()),
            };
            if let Some(&(_, enabled)) = states.iter().find(|(k, _)| k == &key) {
                if i < self.active.len() {
                    self.active[i] = enabled;
                }
            }
        }
    }

    // ========== 属性 ==========

    // ========== 扫描 ==========

    /// 移除不在扫描缓存中的词典（保留在线词典）
    pub fn remove_stale_dicts(&mut self, scanned: &[(String, String, String)]) {
        let scanned_names: std::collections::HashSet<&str> =
            scanned.iter().map(|(_, name, _)| name.as_str()).collect();
        let mut i = 0;
        while i < self.dicts.len() {
            let d = &self.dicts[i];
            if !d.kind().is_online() && !scanned_names.contains(d.name()) {
                self.dicts.remove(i);
                self.active.remove(i);
                self.mdict_paths_index.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// 同步词典列表与扫描缓存：移除过时词典，添加新词典
    pub fn sync_from_cache(&mut self, scanned: &[(String, String, String)]) -> usize {
        self.remove_stale_dicts(scanned);
        self.load_from_cache(scanned)
    }

    /// 从缓存的扫描结果直接加载词典，不遍历文件系统
    pub fn load_from_cache(&mut self, scanned: &[(String, String, String)]) -> usize {
        let mut count = 0;
        for (kind, name, path) in scanned {
            let p = std::path::Path::new(path);
            if !p.exists() {
                continue;
            }
            if self.dicts.iter().any(|d| d.name() == name.as_str()) {
                continue;
            }
            match kind.as_str() {
                "Mdx" => match MdxReader::open(p) {
                    Ok(reader) => {
                        let r: Arc<dyn Dictionary> = Arc::new(MdxDict::new(Arc::new(reader)));
                        let rc = r.clone();
                        self.dicts.push(r);
                        self.active.push(true);
                        self.mdict_paths_index.push(Some(path.clone()));
                        std::thread::spawn(move || {
                            if let Some(mdx) = rc.as_any().downcast_ref::<MdxDict>() {
                                mdx.inner.build_index();
                            }
                        });
                        count += 1;
                    }
                    Err(e) => log::warn!("MDX {}: {}", path, e),
                },
                "StarDict" => {
                    if let Ok(entries) = std::fs::read_dir(&p) {
                        for e in entries.flatten() {
                            let fp = e.path();
                            if fp.is_file() && fp.extension().map(|x| x == "ifo").unwrap_or(false) {
                                if let Ok(r) = DictionaryReader::open(&fp) {
                                    if r.info.bookname == name.as_str() {
                                        self.dicts
                                            .push(Arc::new(StarDictDict { inner: Arc::new(r) }));
                                        self.active.push(true);
                                        self.mdict_paths_index.push(Some(path.clone()));
                                        count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => log::warn!("Unknown dict kind in cache: {}", kind),
            }
        }
        count
    }

    /// 扫描目录发现词典，返回发现结果（不修改自身状态）
    pub fn scan_directories(
        &self,
        dirs: &[PathBuf],
    ) -> Result<Vec<(String, String, String)>, String> {
        let mut found = Vec::new();
        for dir in dirs {
            self.scan_dir_rec(dir, &mut found)?;
        }
        Ok(found)
    }

    fn scan_dir_rec(
        &self,
        dir: &PathBuf,
        found: &mut Vec<(String, String, String)>,
    ) -> Result<(), String> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file() && p.extension().map(|x| x == "ifo").unwrap_or(false) {
                    if let Ok(r) = DictionaryReader::open(&p) {
                        let path_str = p
                            .parent()
                            .map(|x| x.to_string_lossy().to_string())
                            .unwrap_or_default();
                        found.push(("StarDict".into(), r.info.bookname.clone(), path_str));
                    }
                } else if p.is_file() && p.extension().map(|x| x == "mdx").unwrap_or(false) {
                    let path_str = p.to_string_lossy().to_string();
                    let name = match MdxReader::open(&p) {
                        Ok(reader) => reader.name().to_string(),
                        Err(_) => path_str.clone(),
                    };
                    found.push(("Mdx".into(), name, path_str));
                } else if p.is_dir() {
                    self.scan_dir_rec(&p, found)?;
                }
            }
        }
        Ok(())
    }

    pub fn restore_active_by_names(&mut self, names: &[String]) {
        for (i, d) in self.dicts.iter().enumerate() {
            if d.kind() == DictKind::StarDict {
                if let Some(sd) = d.as_any().downcast_ref::<StarDictDict>() {
                    self.active[i] = names.contains(&sd.inner.info.bookname);
                }
            }
        }
    }
}

// ========== Wrapper 类型 ==========

pub struct StarDictDict {
    pub inner: Arc<DictionaryReader>,
}

impl Dictionary for StarDictDict {
    fn name(&self) -> &str {
        &self.inner.info.bookname
    }
    fn word_count(&self) -> usize {
        self.inner.word_count()
    }
    fn kind(&self) -> DictKind {
        DictKind::StarDict
    }
    fn lookup_exact(&self, w: &str) -> Option<ArticleData> {
        self.inner.lookup_exact(w)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn lookup_prefix(&self, word: &str, limit: usize) -> Vec<SearchResult> {
        self.inner.lookup_prefix(word, limit)
    }
    fn lookup_fuzzy(&self, word: &str, threshold: usize, limit: usize) -> Vec<SearchResult> {
        self.inner.lookup_fuzzy(word, threshold, limit)
    }
}

pub struct MdxDict {
    pub inner: Arc<MdxReader>,
    display_name: String,
}

impl MdxDict {
    pub fn new(inner: Arc<MdxReader>) -> Self {
        let display_name = format!("{} (MDX)", inner.name());
        Self { inner, display_name }
    }
}

impl Dictionary for MdxDict {
    fn name(&self) -> &str {
        &self.display_name
    }
    fn word_count(&self) -> usize {
        self.inner.word_count()
    }
    fn kind(&self) -> DictKind {
        DictKind::Mdx
    }
    fn lookup_exact(&self, w: &str) -> Option<ArticleData> {
        self.inner.lookup_exact(w)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn lookup_prefix(&self, word: &str, limit: usize) -> Vec<SearchResult> {
        self.inner.lookup_prefix(word, limit)
    }
    fn lookup_fuzzy(&self, word: &str, threshold: usize, limit: usize) -> Vec<SearchResult> {
        self.inner.lookup_fuzzy(word, threshold, limit)
    }
}

pub struct BaiduDict {
    appid: String,
    apikey: String,
}

impl BaiduDict {
    pub fn new(appid: &str, apikey: &str) -> Self {
        Self {
            appid: appid.to_string(),
            apikey: apikey.to_string(),
        }
    }
}

impl Dictionary for BaiduDict {
    fn name(&self) -> &str {
        "Baidu Translate"
    }
    fn word_count(&self) -> usize {
        0
    }
    fn kind(&self) -> DictKind {
        DictKind::Online("baidu".into())
    }
    fn lookup_exact(&self, w: &str) -> Option<ArticleData> {
        use crate::engine::baidu_client::BaiduTranslateClient;
        if self.appid.is_empty() || self.apikey.is_empty() {
            return None;
        }
        BaiduTranslateClient::new(&self.appid, &self.apikey).translate(w, "zh")
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
