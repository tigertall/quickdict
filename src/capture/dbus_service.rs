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

#[zbus::interface(name = "io.github.tigertall.QuickDict.Translator")]
impl TranslatorService {
    async fn lookup(&self, word: String) -> String {
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
