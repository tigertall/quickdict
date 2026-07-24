use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::application::AppState;
use crate::engine::types::{ArticleData, SearchResult};
use crate::views::content_view::ContentView;
use crate::views::search_bar::SearchBar;
use crate::views::sidebar::Sidebar;

/// 主窗口
pub struct MainWindow {
    window: adw::ApplicationWindow,
    _content_view: Rc<ContentView>,
    search_bar: SearchBar,
    sidebar: Rc<Sidebar>,
}

impl MainWindow {
    /// 刷新侧边栏词典列表
    pub fn refresh_sidebar(&self, dict_manager: &crate::engine::dict_manager::DictManager) {
        let infos = dict_manager.dict_infos();
        self.sidebar.set_dictionaries(&infos);
    }

    pub fn search_word_direct(&self, word: &str, mgr: &crate::engine::dict_manager::DictManager) {
        let word = crate::engine::search_engine::clean_word(word);
        log::info!("search_word_direct: {}", word);
        self.search_bar.entry.set_text(&word);
        self.search_bar.entry.set_position(-1);
        let mut articles = mgr.lookup_local(&word);
        if let Some(a) = mgr.try_online(&word) {
            articles.push(a);
        }
        self._content_view.show_multi_articles(articles);
    }

    pub fn new(app: &adw::Application, state: &AppState) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Dictionary")
            .default_width(state.config.window_width())
            .default_height(state.config.window_height())
            .build();

        let search_bar = SearchBar::new();
        let content_view = Rc::new(ContentView::new());
        let sidebar = Rc::new(Sidebar::new());

        // 设置侧栏词典列表
        let dict_infos = state.dict_manager.borrow().dict_infos();
        sidebar.set_dictionaries(&dict_infos);

        let _prefs_open = state.prefs_open.clone();

        // === 布局：Sidebar | HeaderBar+Content ===
        let header_bar = gtk4::HeaderBar::new();

        search_bar.entry.set_hexpand(true);
        search_bar.entry.set_halign(gtk4::Align::Center);
        search_bar.entry.set_valign(gtk4::Align::Center);
        header_bar.set_title_widget(Some(search_bar.widget()));

        let sidebar_toggle = gtk4::ToggleButton::new();
        sidebar_toggle.set_icon_name("sidebar-show-symbolic");
        sidebar_toggle.add_css_class("flat");
        sidebar_toggle.set_active(false);
        header_bar.pack_start(&sidebar_toggle);

        let menu_btn = gtk4::MenuButton::new();
        menu_btn.set_icon_name("open-menu-symbolic");
        menu_btn.add_css_class("flat");
        {
            let menu = gio::Menu::new();
            menu.append(Some("Preferences"), Some("app.preferences"));
            menu.append(Some("About QuickDict"), Some("app.about"));
            menu_btn.set_menu_model(Some(&menu));
        }
        header_bar.pack_end(&menu_btn);

        // 右侧：ToolbarView(HeaderBar + ContentView)
        let right_toolbar = adw::ToolbarView::new();
        right_toolbar.add_top_bar(&header_bar);
        right_toolbar.set_content(Some(content_view.widget()));

        // 左侧 Sidebar（NavigationSplitView 折叠面板）
        let sidebar_page = adw::NavigationPage::new(sidebar.widget(), "Dictionaries");
        let right_page = adw::NavigationPage::new(&right_toolbar, "Search");

        let split_view = adw::NavigationSplitView::new();
        split_view.set_show_content(true);
        split_view.set_collapsed(true);
        split_view.set_min_sidebar_width(260.0);
        split_view.set_max_sidebar_width(260.0);
        split_view.set_sidebar(Some(&sidebar_page));
        split_view.set_content(Some(&right_page));

        {
            let sv = split_view.clone();
            sidebar_toggle.connect_toggled(move |btn| {
                sv.set_collapsed(!btn.is_active());
            });
        }

        // 同步 collapsed 状态到按钮
        {
            let btn = sidebar_toggle.clone();
            split_view.connect_collapsed_notify(move |sv| {
                btn.set_active(!sv.is_collapsed());
            });
        }

        window.set_content(Some(&split_view));

        // 保存窗口大小
        {
            let config = state.config.clone();
            window.connect_default_width_notify(move |w| {
                config.set_window_width(w.default_width());
            });
            let config2 = state.config.clone();
            window.connect_default_height_notify(move |w| {
                config2.set_window_height(w.default_height());
            });
        }

        // 窗口关闭时保存词典选择状态
        {
            let manager = state.dict_manager.clone();
            let config = state.config.clone();
            window.connect_close_request(move |_| {
                if let Ok(mgr) = manager.try_borrow() {
                    let names = mgr.active_dict_names();
                    if !names.is_empty() {
                        config.set_active_dictionaries(&names);
                        let paths = config.dictionary_paths();
                        config.save_full_state(&paths, &names);
                    }
                }
                glib::Propagation::Proceed
            });
        }

        // 注册 GActions
        let search_entry = search_bar.entry.clone();
        {
            let cv = content_view.clone();
            window.add_action_entries([
                gio::ActionEntry::builder("search-focus")
                    .activate(move |_, _, _| {
                        search_entry.grab_focus();
                    })
                    .build(),
                gio::ActionEntry::builder("go-back")
                    .activate(move |_, _, _| {
                        cv.widget().set_visible_child_name("waiting");
                    })
                    .build(),
            ]);
        }

        // F9 快捷键：ShortcutController(Global) + CallbackAction
        {
            let shortcut_controller = gtk4::ShortcutController::new();
            shortcut_controller.set_scope(gtk4::ShortcutScope::Global);
            let trigger = gtk4::ShortcutTrigger::parse_string("F9").unwrap();
            let sp = split_view.clone();
            let se = search_bar.entry.clone();
            let action = gtk4::CallbackAction::new(move |_, _| {
                sp.set_collapsed(!sp.is_collapsed());
                // 清除 collapsed 切换导致 search entry 重获焦点时的自动全选，光标置尾
                se.set_position(-1);
                glib::Propagation::Proceed
            });
            let shortcut = gtk4::Shortcut::builder()
                .trigger(&trigger)
                .action(&action)
                .build();
            shortcut_controller.add_shortcut(shortcut);
            window.add_controller(shortcut_controller);
        }

        // Ctrl+L 快捷键：ShortcutController(Global) 确保搜索框不消费
        {
            let shortcut_controller = gtk4::ShortcutController::new();
            shortcut_controller.set_scope(gtk4::ShortcutScope::Global);
            let trigger = gtk4::ShortcutTrigger::parse_string("<Control>L").unwrap();
            let entry = search_bar.entry.clone();
            let action = gtk4::CallbackAction::new(move |_, _| {
                entry.grab_focus();
                glib::Propagation::Proceed
            });
            let shortcut = gtk4::Shortcut::builder()
                .trigger(&trigger)
                .action(&action)
                .build();
            shortcut_controller.add_shortcut(shortcut);
            window.add_controller(shortcut_controller);
        }

        // === 连接信号 ===
        let current_results: Rc<RefCell<Vec<SearchResult>>> = Rc::new(RefCell::new(Vec::new()));

        // 搜索
        {
            let manager = state.dict_manager.clone();
            let engine = state.search_engine.clone();
            let results_cache = current_results.clone();
            let cv = content_view.clone();

            search_bar.connect_search(move |query| {
                let query = crate::engine::search_engine::clean_word(query);
                if query.is_empty() {
                    return;
                }
                let (res, articles) = {
                    let mgr = match manager.try_borrow() {
                        Ok(m) => m,
                        Err(_) => {
                            log::warn!("DictManager already borrowed, skipping search");
                            return;
                        }
                    };
                    let res = engine.search(&query, &mgr);
                    let mut articles: Vec<ArticleData> = Vec::new();
                    for dict in mgr.enabled() {
                        if let Some(r) = res
                            .iter()
                            .find(|r| r.score >= 1.0 && r.dict_name == dict.name())
                        {
                            if let Some(article) = dict.lookup_exact(&r.word) {
                                articles.push(article);
                            }
                        }
                    }
                    // 在线翻译（同步，与 search_word_direct 行为一致）
                    if let Some(article) = mgr.try_online(&query) {
                        articles.push(article);
                    }
                    (res, articles)
                };

                // 更新结果缓存
                match results_cache.try_borrow_mut() {
                    Ok(mut r) => *r = res.clone(),
                    Err(_) => {
                        log::warn!("results already borrowed, skipping update");
                        return;
                    }
                };

                // 显示结果
                if res.is_empty() && articles.is_empty() {
                    cv.widget().set_visible_child_name("waiting");
                } else if !articles.is_empty() {
                    cv.show_multi_articles(articles);
                } else {
                    cv.show_results(&res);
                }
            });
        }

        // 搜索结果选中 → 渲染文章
        {
            let manager = state.dict_manager.clone();
            let cv = content_view.clone();
            let cv2 = cv.clone();
            let cv3 = cv.clone();
            let wl = cv.word_list().results().clone();
            cv.word_list().connect_selected(move |idx| {
                if cv3.is_updating() {
                    return;
                }
                let res = match wl.try_borrow() {
                    Ok(r) => r,
                    Err(_) => return,
                };
                if let Some(selected) = res.get(idx) {
                    let mgr = match manager.try_borrow() {
                        Ok(m) => m,
                        Err(_) => return,
                    };
                    for dict in mgr.enabled() {
                        if dict.name() == selected.dict_name {
                            if let Some(article) = dict.lookup_exact(&selected.word) {
                                cv2.show_single_article(&article);
                            }
                            break;
                        }
                    }
                }
            });
        }

        // 侧栏词典切换
        {
            let manager = state.dict_manager.clone();
            let config = state.config.clone();
            let sb = sidebar.clone();
            sidebar.connect_toggle(move |idx, enabled| {
                match manager.try_borrow_mut() {
                    Ok(mut mgr) => {
                        // Map sidebar idx to global dicts idx (online dicts are filtered out)
                        let infos = mgr.dict_infos();
                        let global_idx = if idx < infos.len() {
                            let info = &infos[idx];
                            mgr.all()
                                .iter()
                                .position(|d| d.name() == info.name && d.kind() == info.kind)
                        } else {
                            None
                        };
                        if let Some(gi) = global_idx {
                            mgr.toggle_dict(gi, enabled);
                        }
                        let names = mgr.active_dict_names();
                        let states = mgr.export_active_states();
                        drop(mgr);
                        config.set_active_dictionaries(&names);
                        config.save_dict_active_states(&states);
                        let paths = config.dictionary_paths();
                        config.save_full_state(&paths, &names);
                    }
                    Err(e) => {
                        log::warn!("Failed to borrow dict_manager for toggle: {:?}", e);
                    }
                }
                sb.update_stats();
            });
        }

        // 百度翻译开关（通过 DictManager 统一管理）
        {
            let manager = state.dict_manager.clone();
            let config_baidu = state.config.clone();
            sidebar.connect_baidu_toggle(move |enabled| {
                if let Ok(mgr) = manager.try_borrow() {
                    if let Some(bi) = mgr.online_idx("baidu") {
                        drop(mgr);
                        if let Ok(mut mgr2) = manager.try_borrow_mut() {
                            mgr2.toggle_dict(bi, enabled);
                            let states = mgr2.export_active_states();
                            drop(mgr2);
                            config_baidu.save_dict_active_states(&states);
                        }
                    }
                }
            });
        }

        // Poll for background dict loading completion
        {
            let dm_clone = state.dict_manager.clone();
            let dl_clone = state.dicts_loaded.clone();
            let sidebar_clone = sidebar.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                if dl_clone.get() {
                    if let Ok(mgr) = dm_clone.try_borrow() {
                        let infos = mgr.dict_infos();
                        sidebar_clone.set_dictionaries(&infos);
                        let bd_enabled = mgr
                            .online_idx("baidu")
                            .map(|i| mgr.is_active(i))
                            .unwrap_or(true);
                        sidebar_clone.sync_baidu_state(bd_enabled);
                    }
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        }

        // Set up internal link handler (entry://word → lookup)
        {
            let sb_entry = search_bar.entry.clone();
            let cv_link = content_view.clone();
            let manager = state.dict_manager.clone();
            let cv = content_view.clone();
            cv.set_link_handler(Rc::new(move |word| {
                let word = crate::engine::search_engine::clean_word(&word);
                if word.is_empty() {
                    return;
                }
                sb_entry.set_text(&word);
                sb_entry.set_position(-1);
                if let Ok(mgr) = manager.try_borrow() {
                    let mut articles = mgr.lookup_local(&word);
                    if let Some(a) = mgr.try_online(&word) {
                        articles.push(a);
                    }
                    cv_link.show_multi_articles(articles);
                }
            }));
        }

        // Focus search entry on startup
        search_bar.entry.grab_focus();

        Self {
            window,
            _content_view: content_view,
            search_bar: search_bar,
            sidebar: sidebar,
        }
    }

    pub fn present(&self) {
        self.window.present();
    }
}
