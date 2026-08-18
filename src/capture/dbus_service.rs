/// D-Bus translation service for GNOME Shell extension.
/// Lookup requests are forwarded to the main GTK thread via mpsc channel.
use std::sync::{Mutex, mpsc};
use once_cell::sync::Lazy;
use zbus::Connection;
use futures_util::StreamExt;

type LookupRequest = (String, tokio::sync::oneshot::Sender<String>);

static LOOKUP_TX: Lazy<Mutex<Option<mpsc::Sender<LookupRequest>>>> = Lazy::new(|| Mutex::new(None));

pub fn set_lookup_channel(tx: mpsc::Sender<LookupRequest>) {
    *LOOKUP_TX.lock().unwrap() = Some(tx);
}

struct TranslatorService;

impl TranslatorService {
    async fn lookup_inner(word: String) -> String {
        let otx = {
            let tx_guard = LOOKUP_TX.lock().unwrap();
            if let Some(ref tx) = *tx_guard {
                let (otx, orx) = tokio::sync::oneshot::channel();
                let _ = tx.send((word, otx));
                Some(orx)
            } else {
                None
            }
        };
        if let Some(orx) = otx {
            match orx.await {
                Ok(result) => result,
                Err(_) => "[]".into(),
            }
        } else {
            "[]".into()
        }
    }
}

#[zbus::interface(name = "io.github.tigertall.QuickDict.Translator")]
impl TranslatorService {
    async fn lookup(&self, word: String) -> String {
        Self::lookup_inner(word).await
    }

    /// OCR lookup: image_base64 is a PNG of the captured region,
    /// cursor_x/cursor_y are relative to the region's top-left corner in
    /// logical coordinates; cap_w/cap_h are the region's logical size.
    /// Returns a JSON object {"word": ..., "results": [...]} so the caller
    /// can show the recognized word and open it in the main window.
    async fn lookup_image(&self, image_base64: String, cursor_x: i32, cursor_y: i32, cap_w: i32, cap_h: i32) -> String {
        let word = tokio::task::spawn_blocking(move || {
            crate::capture::ocr::ocr_word_at(&image_base64, cursor_x, cursor_y, cap_w, cap_h)
        })
        .await
        .unwrap_or_default();
        let Some(word) = word else {
            log::warn!("[dbus_svc] OCR: no word at ({}, {})", cursor_x, cursor_y);
            return serde_json::json!({"word": serde_json::Value::Null, "results": []}).to_string();
        };
        log::info!("[dbus_svc] OCR word: \"{}\"", word);
        // lookup_inner forwards to the main thread, which cleans the word
        // (stripping punctuation) and returns {"word": cleaned, "results": [...]}.
        let result = Self::lookup_inner(word.clone()).await;
        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&result) {
            if resp.get("results").and_then(|r| r.as_array()).is_some_and(|a| a.is_empty()) {
                log::warn!("[dbus_svc] OCR word \"{}\": no dictionary results", word);
            }
        }
        result
    }
}

pub fn start_dbus_service() {
    std::thread::spawn(|| {
        let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => { log::warn!("[dbus_svc] tokio: {}", e); return; }
        };
        rt.block_on(async {
            let conn = match Connection::session().await {
                Ok(c) => c,
                Err(e) => { log::warn!("[dbus_svc] D-Bus: {}", e); return; }
            };
            let svc = TranslatorService;
            if let Err(e) = conn.object_server().at("/io/github/tigertall/QuickDict/Translator", svc).await {
                log::warn!("[dbus_svc] register: {}", e); return;
            }
            if let Err(e) = conn.request_name("io.github.tigertall.QuickDict.Translator").await {
                log::warn!("[dbus_svc] request_name: {}", e); return;
            }
            log::info!("[dbus_svc] D-Bus service active");
            let mut stream = zbus::MessageStream::from(&conn);
            while (stream.next().await).is_some() {}
        });
    });
}
