use gio::prelude::*;

/// GSettings 键名常量
const SCHEMA_ID: &str = "io.github.tigertall.QuickDict";
const KEY_DICT_PATHS: &str = "dictionary-paths";
const KEY_MAX_RESULTS: &str = "max-results";
const KEY_FUZZY_THRESHOLD: &str = "fuzzy-threshold";
const KEY_WINDOW_WIDTH: &str = "window-width";
const KEY_WINDOW_HEIGHT: &str = "window-height";
const KEY_DICT_ORDER: &str = "dict-order";
const KEY_BAIDU_APPID: &str = "baidu-appid";
const KEY_BAIDU_APIKEY: &str = "baidu-apikey";
const KEY_SCANNED_DICTS: &str = "scanned-dicts";
const KEY_DICT_ACTIVE_STATES: &str = "dict-active-states";

/// GSettings 配置封装
#[derive(Debug, Clone)]
pub struct Config {
    settings: gio::Settings,
}

impl Config {
    pub fn new() -> Self {
        let settings = gio::Settings::new(SCHEMA_ID);
        Self { settings }
    }

    /// 词典搜索路径
    pub fn dictionary_paths(&self) -> Vec<String> {
        let v = self.settings.strv(KEY_DICT_PATHS);
        v.iter().map(|s| s.to_string()).collect()
    }

    pub fn set_dictionary_paths(&self, paths: &[String]) {
        let _ = self.settings.set_strv(KEY_DICT_PATHS, paths);
    }

    /// 最大搜索结果数
    pub fn max_results(&self) -> i32 {
        self.settings.int(KEY_MAX_RESULTS)
    }

    pub fn set_max_results(&self, val: i32) {
        let _ = self.settings.set_int(KEY_MAX_RESULTS, val);
    }

    /// 模糊匹配阈值
    pub fn fuzzy_threshold(&self) -> i32 {
        self.settings.int(KEY_FUZZY_THRESHOLD)
    }

    pub fn set_fuzzy_threshold(&self, val: i32) {
        let _ = self.settings.set_int(KEY_FUZZY_THRESHOLD, val);
    }

    /// 窗口宽度
    pub fn window_width(&self) -> i32 {
        self.settings.int(KEY_WINDOW_WIDTH)
    }

    pub fn set_window_width(&self, val: i32) {
        let _ = self.settings.set_int(KEY_WINDOW_WIDTH, val);
    }

    /// 窗口高度
    pub fn window_height(&self) -> i32 {
        self.settings.int(KEY_WINDOW_HEIGHT)
    }

    pub fn set_window_height(&self, val: i32) {
        let _ = self.settings.set_int(KEY_WINDOW_HEIGHT, val);
    }

    /// 活跃词典名称列表（GSettings）
    pub fn active_dictionaries(&self) -> Vec<String> {
        // Derived from dict-active-states: filter enabled StarDict entries
        self.load_dict_active_states().iter()
            .filter(|(_, v)| *v)
            .filter(|(k, _)| k.starts_with("StarDict:"))
            .map(|(k, _)| k.trim_start_matches("StarDict:").to_string())
            .collect()
    }

    pub fn set_active_dictionaries(&self, names: &[String]) {
        // Update StarDict entries in dict-active-states, preserve others
        let mut states = self.load_dict_active_states();
        for (k, v) in states.iter_mut() {
            if k.starts_with("StarDict:") {
                let name = k.trim_start_matches("StarDict:");
                *v = names.contains(&name.to_string());
            }
        }
        self.save_dict_active_states(&states);
    }

    // === 扫描结果缓存 ===

    /// 格式: Vec<(kind, name, path)>
    pub fn scanned_dicts(&self) -> Vec<(String, String, String)> {
        self.settings.strv(KEY_SCANNED_DICTS).iter()
            .filter_map(|s| {
                let s = s.to_string();
                let mut parts = s.splitn(2, '=');
                let key = parts.next()?;
                let path = parts.next()?;
                if let Some(pos) = key.rfind(':') {
                    let kind = key[..pos].to_string();
                    let name = key[pos+1..].to_string();
                    Some((kind, name, path.to_string()))
                } else { None }
            })
            .collect()
    }

    pub fn set_scanned_dicts(&self, dicts: &[(String, String, String)]) {
        let v: Vec<String> = dicts.iter()
            .map(|(kind, name, path)| format!("{}:{}={}", kind, name, path))
            .collect();
        let strs: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
        let _ = self.settings.set_strv(KEY_SCANNED_DICTS, &strs[..]);
    }

    pub fn load_dict_active_states(&self) -> Vec<(String, bool)> {
        self.settings.strv(KEY_DICT_ACTIVE_STATES).iter()
            .filter_map(|s| {
                let s = s.to_string();
                if let Some(pos) = s.rfind('=') {
                    let key = s[..pos].to_string();
                    let val = &s[pos+1..];
                    Some((key, val == "true"))
                } else { None }
            })
            .collect()
    }

    pub fn save_dict_active_states(&self, states: &[(String, bool)]) {
        let v: Vec<String> = states.iter()
            .map(|(k, v)| format!("{}={}", k, if *v { "true" } else { "false" }))
            .collect();
        let strs: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
        let _ = self.settings.set_strv(KEY_DICT_ACTIVE_STATES, &strs[..]);
    }

    // === 百度翻译 ===

    pub fn baidu_appid(&self) -> String {
        self.settings.string(KEY_BAIDU_APPID).to_string()
    }

    pub fn set_baidu_appid(&self, val: &str) {
        let _ = self.settings.set_string(KEY_BAIDU_APPID, val);
    }

    pub fn baidu_apikey(&self) -> String {
        self.settings.string(KEY_BAIDU_APIKEY).to_string()
    }

    pub fn set_baidu_apikey(&self, val: &str) {
        let _ = self.settings.set_string(KEY_BAIDU_APIKEY, val);
    }

    pub fn baidu_credentials(&self) -> (String, String) {
        (self.baidu_appid(), self.baidu_apikey())
    }

    pub fn set_baidu_credentials(&self, appid: &str, apikey: &str) {
        self.set_baidu_appid(appid);
        self.set_baidu_apikey(apikey);
    }

    // === 词典顺序 ===

    pub fn save_dict_order(&self, order: &[String]) {
        let strs: Vec<&str> = order.iter().map(|s| s.as_str()).collect();
        let _ = self.settings.set_strv(KEY_DICT_ORDER, &strs[..]);
    }

    pub fn load_dict_order(&self) -> Vec<String> {
        self.settings.strv(KEY_DICT_ORDER).iter().map(|s| s.to_string()).collect()
    }

    // === 便捷方法 ===

    pub fn save_full_state(&self, paths: &[String], active: &[String]) {
        self.set_dictionary_paths(paths);
        self.set_active_dictionaries(active);
    }
}
