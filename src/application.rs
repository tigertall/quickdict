use gtk4::prelude::*;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::config::Config;
use crate::engine::dict_manager::DictManager;
use crate::engine::search_engine::{SearchConfig, SearchEngine};
use crate::window::MainWindow;

/// 应用状态（主窗口可能隐藏到后台）
pub struct AppState {
    pub config: Config,
    pub dict_manager: Rc<RefCell<DictManager>>,
    pub search_engine: SearchEngine,
    pub prefs_open: Rc<std::cell::Cell<bool>>,
    pub dicts_loaded: Rc<std::cell::Cell<bool>>,
    /// 词典变更回调（由 MainWindow 设置，Preferences 触发）
    pub on_dicts_changed: Rc<RefCell<Option<Box<dyn Fn()>>>>,
}

/// 主 Application
pub struct DictionaryApplication {
    app: adw::Application,
}

impl DictionaryApplication {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id("io.github.tigertall.QuickDict")
            .build();

        let config = Config::new();
        let dict_manager = Rc::new(RefCell::new(DictManager::new()));
        let search_engine = SearchEngine::new(SearchConfig {
            max_results: config.max_results() as usize,
            fuzzy_threshold: config.fuzzy_threshold() as usize,
            ..Default::default()
        });

        let state = Rc::new(AppState {
            config: config.clone(),
            dict_manager,
            search_engine,
            prefs_open: Rc::new(std::cell::Cell::new(false)),
            dicts_loaded: Rc::new(std::cell::Cell::new(false)),
            on_dicts_changed: Rc::new(RefCell::new(None)),
        });

        let main_window: Rc<RefCell<Option<MainWindow>>> = Rc::new(RefCell::new(None));

        // D-Bus translation service: channel to forward queries to main thread
        let (lookup_tx, lookup_rx) = std::sync::mpsc::channel::<(String, tokio::sync::oneshot::Sender<String>)>();
        crate::capture::dbus_service::set_lookup_channel(lookup_tx);
        crate::capture::dbus_service::start_dbus_service();

        // Poll lookup requests on main thread
        {
            let dm = state.dict_manager.clone();
            let rx: Rc<RefCell<std::sync::mpsc::Receiver<(String, tokio::sync::oneshot::Sender<String>)>>> = Rc::new(RefCell::new(lookup_rx));
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                if let Ok(r) = rx.try_borrow_mut() {
                    while let Ok((word, sender)) = r.try_recv() {
                        // App filter now handled by extension
                        let cleaned = crate::engine::search_engine::clean_word(&word);
                        let articles = {
                            if let Ok(mgr) = dm.try_borrow() {
                                let mut a = mgr.lookup_local(&cleaned);
                                if a.is_empty() {
                                    if let Some(oa) = mgr.try_online(&cleaned) {
                                        a.push(oa);
                                    }
                                }
                                a
                            } else {
                                Vec::new()
                            }
                        };
                        let items: Vec<serde_json::Value> = articles.iter().map(|a| {
                            let text = if a.is_html {
                                crate::views::content_view::html_to_pango_markup(&a.raw_text)
                            } else {
                                a.raw_text.clone()
                            };
                            serde_json::json!({"name": a.dict_name, "text": text})
                        }).collect();
                        let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".into());
                        let _ = sender.send(json);
                    }
                }
                glib::ControlFlow::Continue
            });
        }

        // 设置 GActions
        {
            // app.preferences
            let config_for_pref = config.clone();
            let dm_for_pref = state.dict_manager.clone();
            let po = state.prefs_open.clone();
            let on_change = state.on_dicts_changed.clone();
            let app_for_pref = app.clone();
            let pref_action = gio::SimpleAction::new("preferences", None);
            pref_action.connect_activate(move |_, _| {
                po.set(true);
                let pref_win = crate::preferences::PreferencesWindow::new(
                    &config_for_pref,
                    dm_for_pref.clone(),
                    Box::new({
                        let on_change = on_change.clone();
                        move || {
                            if let Some(ref f) = *on_change.borrow() {
                                f();
                            }
                        }
                    }),
                );
                // Set parent for modal behavior
                if let Some(main_win) = app_for_pref.active_window() {
                    pref_win.set_parent(&main_win);
                }
                {
                    let po = po.clone();
                    pref_win.widget().connect_close_request(move |_| {
                        po.set(false);
                        glib::Propagation::Proceed
                    });
                }
                pref_win.present();
            });
            app.add_action(&pref_action);

            // app.about
            let about_action = gio::SimpleAction::new("about", None);
            about_action.connect_activate(move |_, _| {
                let about = adw::AboutDialog::builder()
                    .application_name("QuickDict")
                    .application_icon("io.github.tigertall.QuickDict")
                    .developer_name("QuickDict Team with Freedom")
                    .version(env!("CARGO_PKG_VERSION"))
                    .license_type(gtk4::License::MitX11)
                    .website("https://github.com/tigertall/quickdict")
                    .issue_url("https://github.com/tigertall/quickdict/issues")
                    .build();
                about.present(None::<&gtk4::Window>);
            });
            app.add_action(&about_action);

            // app.search-word (app-level, used by extension's ActivateAction)
            let search_word_action = gio::SimpleAction::new("search-word", Some(&String::static_variant_type()));
            let mw_for_search = main_window.clone();
            let dm_for_search = state.dict_manager.clone();
            search_word_action.connect_activate(move |_, param| {
                if let Some(word) = param.and_then(|p| p.str().map(|s| s.to_string())) {
                    log::info!("search-word action: {}", word);
                    if let Some(ref w) = *mw_for_search.borrow() {
                        if let Ok(mgr) = dm_for_search.try_borrow() {
                            w.search_word_direct(&word, &mgr);
                        }
                    }
                }
            });
            app.add_action(&search_word_action);
        }

        // 统一处理 activate 和 open 信号，确保应用菜单与活动视图行为一致
        let activate_fn: Rc<dyn Fn(&adw::Application)> = {
            let state = state.clone();
            let mw = main_window.clone();
            Rc::new(move |app| {
                // 单例窗口：已存在则直接激活
                if let Some(ref win) = *mw.borrow() {
                    win.present();
                    return;
                }

                // 延迟扫描：先显示 UI，后台加载词典
                let dl = state.dicts_loaded.clone();
                let dm = state.dict_manager.clone();
                let saved_active = state.config.active_dictionaries();
                let dict_paths: Vec<std::path::PathBuf> = {
                    let p = state.config.dictionary_paths();
                    if p.is_empty() { vec![] } else { p.iter().map(std::path::PathBuf::from).collect() }
                };

                let config = state.config.clone();
                glib::idle_add_local_once(move || {
                    let mut mgr = match dm.try_borrow_mut() {
                        Ok(m) => m,
                        Err(_) => return,
                    };
                    // Register Baidu online dict first (index 0)
                    mgr.add_online_dict(Arc::new(crate::engine::dict_manager::BaiduDict));
                    // Load dicts from scanned cache, or scan if empty
                    let scanned = config.scanned_dicts();
                    if !scanned.is_empty() {
                        mgr.load_from_cache(&scanned);
                    } else if !dict_paths.is_empty() {
                        // First run: scan and cache
                        if let Ok(found) = mgr.scan_directories(&dict_paths) {
                            if !found.is_empty() {
                                config.set_scanned_dicts(&found);
                                mgr.load_from_cache(&found);
                            }
                        }
                    }
                    if !saved_active.is_empty() { mgr.restore_active_by_names(&saved_active); }
                    // Restore all dict active states (including online + MDX)
                    let dict_states = config.load_dict_active_states();
                    if !dict_states.is_empty() { mgr.import_active_states(&dict_states); }
                    // Restore saved dictionary order
                    let saved_order = config.load_dict_order();
                    if !saved_order.is_empty() {
                        mgr.reorder_by(&saved_order);
                    }
                    drop(mgr);
                    dl.set(true);
                });

                // 创建主窗口（立即显示）
                let window = MainWindow::new(app, &state);

                // 设置词典变更回调（Preferences 中增删词典后刷新侧边栏）
                {
                    let dm = state.dict_manager.clone();
                    let mw = mw.clone();
                    *state.on_dicts_changed.borrow_mut() = Some(Box::new(move || {
                        if let Some(ref w) = *mw.borrow() {
                            if let Ok(mgr) = dm.try_borrow() {
                                eprintln!("sidebar refresh: dicts={:?}", mgr.dict_infos().iter().map(|d|d.name.clone()).collect::<Vec<_>>());
                                w.refresh_sidebar(&mgr);
                            } else {
                                eprintln!("sidebar refresh FAILED: dm borrowed");
                            }
                        }
                    }));
                }
                window.present();
                *mw.borrow_mut() = Some(window);
            })
        };

        app.connect_activate({
            let f = activate_fn.clone();
            move |app| f(app)
        });
        app.connect_open({
            let f = activate_fn.clone();
            move |app, _files, _hint| f(app)
        });

        Self {
            app,
        }
    }

    pub fn run(&self) {
        self.app.run();
    }
}
