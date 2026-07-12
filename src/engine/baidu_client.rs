use serde::Deserialize;

use crate::engine::types::ArticleData;

/// 百度大模型文本翻译 API 客户端（Bearer Token 鉴权）
pub struct BaiduTranslateClient {
    appid: String,
    apikey: String,
}

/// API 响应结构（trans_result 在顶层）
#[derive(Debug, Deserialize)]
struct TranslateResponse {
    #[serde(rename = "error_code")]
    error_code: Option<String>,
    #[serde(rename = "error_msg")]
    error_msg: Option<String>,
    from: Option<String>,
    trans_result: Option<Vec<TransItem>>,
}

#[derive(Debug, Deserialize)]
struct TransItem {
    dst: Option<String>,
}

impl BaiduTranslateClient {
    pub fn new(appid: &str, apikey: &str) -> Self {
        Self {
            appid: appid.to_string(),
            apikey: apikey.to_string(),
        }
    }

    /// 调用百度大模型翻译 API（Bearer Token 鉴权）
    pub fn translate(&self, text: &str, to_lang: &str) -> Option<ArticleData> {
        let body = serde_json::json!({
            "appid": self.appid,
            "q": text,
            "from": "auto",
            "to": to_lang,
            "model_type": "llm",
        });

        let body_str = body.to_string();

        let response = match ureq::post("https://fanyi-api.baidu.com/ait/api/aiTextTranslate")
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", self.apikey))
            .send_string(&body_str)
        {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Baidu Translate request failed: {}", e);
                return None;
            }
        };

        let resp: TranslateResponse = match response.into_string() {
            Ok(body) => match serde_json::from_str(&body) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("Baidu Translate parse error: {} body={}", e, body);
                    return None;
                }
            },
            Err(e) => {
                log::warn!("Baidu Translate read error: {}", e);
                return None;
            }
        };

        if let Some(code) = &resp.error_code {
            if code != "0" && !code.is_empty() {
                log::warn!(
                    "Baidu Translate error: code={} msg={:?}",
                    code,
                    resp.error_msg
                );
                return None;
            }
        }

        let items = resp.trans_result?;

        let translated: Vec<String> = items
            .iter()
            .filter_map(|item| item.dst.as_ref())
            .cloned()
            .collect();

        if translated.is_empty() {
            return None;
        }

        let combined = translated.join(" / ");
        let from = resp.from.unwrap_or_else(|| "auto".into());

        Some(ArticleData {
            dict_name: format!("Baidu Translate ({})", from),
            raw_text: combined,
            is_html: false,
        })
    }
}
