use gtk4::prelude::*;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use crate::config::Config;
use crate::engine::dict_manager::DictManager;

/// 偏好设置窗口（单页面，分组标题）
pub struct PreferencesWindow {
    window: adw::PreferencesWindow,
    dict_rows: gtk4::ListBox,
    path_rows: gtk4::ListBox,
    pub(crate) rebuild_fn: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    config: Config,
}

/// 重建词典行的独立实现（避免 self borrow 冲突）
fn rebuild_dict_rows_impl(rows: &gtk4::ListBox, dm: &Rc<RefCell<DictManager>>, _cb: &Rc<dyn Fn()>, rebuild_all: &Rc<dyn Fn()>, config: &Config) {
    while let Some(child) = rows.first_child() { rows.remove(&child); }
    let infos = dm.borrow().dict_infos();
    if infos.is_empty() {
        let empty_label = gtk4::Label::new(Some("No dictionaries loaded."));
        empty_label.add_css_class("dim-label");
        empty_label.set_xalign(0.0);
        empty_label.set_margin_start(12);
        empty_label.set_margin_top(6);
        empty_label.set_margin_bottom(6);
        let empty_row = gtk4::ListBoxRow::new();
        empty_row.set_child(Some(&empty_label));
        rows.append(&empty_row);
        return;
    }
    for (i, info) in infos.iter().enumerate() {
        let row = adw::ActionRow::new();
        row.set_title(&info.name);
        row.set_subtitle(&format!("{} words  ·  {}", info.word_count, info.path));
        if i > 0 {
            let up_btn = gtk4::Button::from_icon_name("go-up-symbolic");
            up_btn.add_css_class("flat"); up_btn.set_valign(gtk4::Align::Center);
            let dm2 = dm.clone(); let ra = rebuild_all.clone();
            let info_name = info.name.clone();
            let info_kind = info.kind.clone();
            up_btn.connect_clicked(move |_| {
                eprintln!("BTN-UP i={}", i);
                if let Ok(mut mgr) = dm2.try_borrow_mut() {
                    let global_i = mgr.all().iter().position(|d| d.name() == info_name && d.kind() == info_kind).unwrap_or(i);
                    eprintln!("move_up({}) from {:?}", global_i, mgr.dict_infos().iter().map(|d|d.name.clone()).collect::<Vec<_>>());
                    mgr.move_dict_up(global_i);
                    eprintln!("  -> {:?}", mgr.dict_infos().iter().map(|d|d.name.clone()).collect::<Vec<_>>());
                }
                else { eprintln!("move_up({}) FAILED borrow", i); }
                eprintln!("BTN-UP calling ra()");
                ra();
                eprintln!("BTN-UP ra() done");
            });
            if i + 1 < infos.len() {
                let down_btn = gtk4::Button::from_icon_name("go-down-symbolic");
                down_btn.add_css_class("flat"); down_btn.set_valign(gtk4::Align::Center);
                let dm3 = dm.clone(); let ra = rebuild_all.clone();
                let info_name = info.name.clone();
                let info_kind = info.kind.clone();
                down_btn.connect_clicked(move |_| {
                    if let Ok(mut mgr) = dm3.try_borrow_mut() {
                        let global_i = mgr.all().iter().position(|d| d.name() == info_name && d.kind() == info_kind).unwrap_or(i);
                        eprintln!("move_down({}) from {:?}", global_i, mgr.dict_infos().iter().map(|d|d.name.clone()).collect::<Vec<_>>());
                        mgr.move_dict_down(global_i);
                        eprintln!("  -> {:?}", mgr.dict_infos().iter().map(|d|d.name.clone()).collect::<Vec<_>>());
                    }
                    else { eprintln!("move_down({}) FAILED borrow", i); }
                    ra();
                });
                let arrow_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                arrow_box.add_css_class("linked");
                arrow_box.append(&up_btn);
                arrow_box.append(&down_btn);
                row.add_suffix(&arrow_box);
            } else {
                row.add_suffix(&up_btn);
            }
        } else if i + 1 < infos.len() {
            let down_btn = gtk4::Button::from_icon_name("go-down-symbolic");
            down_btn.add_css_class("flat"); down_btn.set_valign(gtk4::Align::Center);
            let dm3 = dm.clone(); let ra = rebuild_all.clone();
            let info_name = info.name.clone();
            let info_kind = info.kind.clone();
            down_btn.connect_clicked(move |_| {
                if let Ok(mut mgr) = dm3.try_borrow_mut() {
                    let global_i = mgr.all().iter().position(|d| d.name() == info_name && d.kind() == info_kind).unwrap_or(i);
                    eprintln!("move_down({}) from {:?}", global_i, mgr.dict_infos().iter().map(|d|d.name.clone()).collect::<Vec<_>>());
                    mgr.move_dict_down(global_i);
                    eprintln!("  -> {:?}", mgr.dict_infos().iter().map(|d|d.name.clone()).collect::<Vec<_>>());
                }
                else { eprintln!("move_down({}) FAILED borrow", i); }
                ra();
            });
            row.add_suffix(&down_btn);
        }
        let list_row = gtk4::ListBoxRow::new();
        list_row.set_child(Some(&row));
        rows.append(&list_row);
    }
    rows.queue_resize();
    // Persist dict order after every rebuild
    if let Ok(mgr) = dm.try_borrow() {
        let order = mgr.export_order();
        config.save_dict_order(&order);
    }
}

fn rebuild_path_rows(rows: &gtk4::ListBox, config: &Config, _cb: &Rc<dyn Fn()>) {
    while let Some(child) = rows.first_child() { rows.remove(&child); }
    let paths = config.dictionary_paths();
    if paths.is_empty() {
        let empty_label = gtk4::Label::new(Some("No directories added."));
        empty_label.add_css_class("dim-label");
        empty_label.set_xalign(0.0);
        empty_label.set_margin_start(12);
        empty_label.set_margin_top(6);
        empty_label.set_margin_bottom(6);
        let empty_row = gtk4::ListBoxRow::new();
        empty_row.set_child(Some(&empty_label));
        rows.append(&empty_row);
        return;
    }
    for (_i, path) in paths.iter().enumerate() {
        let row = adw::ActionRow::new();
        row.set_title(path);
        row.set_subtitle("Dictionary directory");
        let del_btn = gtk4::Button::from_icon_name("user-trash-symbolic");
        del_btn.add_css_class("flat"); del_btn.add_css_class("circular");
        del_btn.set_valign(gtk4::Align::Center);
        {
            let cfg = config.clone();
            let path_c = path.clone();
            let rows_c = rows.clone();
            let cb = _cb.clone();
            del_btn.connect_clicked(move |_| {
                let mut all = cfg.dictionary_paths();
                all.retain(|p| p != &path_c);
                cfg.set_dictionary_paths(&all);
                rebuild_path_rows(&rows_c, &cfg, &cb);
            });
        }
        row.add_suffix(&del_btn);
        let list_row = gtk4::ListBoxRow::new();
        list_row.set_child(Some(&row));
        rows.append(&list_row);
    }
    rows.queue_resize();
}

impl PreferencesWindow {
    /// 创建偏好设置窗口
    /// `on_dicts_changed`: 词典增删后回调，用于同步主界面侧边栏
    pub fn new(config: &Config, dm: Rc<RefCell<DictManager>>, on_dicts_changed: Box<dyn Fn()>) -> Self {
        let on_dicts_changed: Rc<dyn Fn()> = Rc::from(on_dicts_changed);
        let window = adw::PreferencesWindow::new();
        let page = adw::PreferencesPage::new();

        let rfn_cell: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));

        // === Dictionary Paths ===
        let path_group = adw::PreferencesGroup::new();
        path_group.set_title("Dictionary Paths");
        path_group.set_description(Some("Directories to scan for dictionaries."));

        let path_rows = gtk4::ListBox::new();
        path_rows.add_css_class("boxed-list");
        path_rows.set_selection_mode(gtk4::SelectionMode::None);
        path_group.add(&path_rows);

        let add_dir_btn = gtk4::Button::with_label("Add Directory...");
        add_dir_btn.add_css_class("suggested-action");

        let scan_all_btn = gtk4::Button::with_label("Scan Directories");
        scan_all_btn.add_css_class("suggested-action");

        let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        btn_box.set_margin_top(6);
        btn_box.set_margin_bottom(6);
        btn_box.append(&add_dir_btn);
        btn_box.append(&scan_all_btn);
        path_group.add(&btn_box);

        // Need dict_rows early so scan callback can rebuild it
        let dict_rows = gtk4::ListBox::new();
        dict_rows.add_css_class("boxed-list");
        dict_rows.set_selection_mode(gtk4::SelectionMode::None);

        {
            let config = config.clone();
            let cb = on_dicts_changed.clone();
            let path_rows_c = path_rows.clone();
            add_dir_btn.connect_clicked(move |_| {
                let dialog = gtk4::FileDialog::new();
                dialog.set_title("Select Dictionary Directory");
                let config = config.clone();
                let cb = cb.clone();
                let path_rows_c = path_rows_c.clone();
                dialog.select_folder(None::<&gtk4::Window>, gio::Cancellable::NONE, move |result| {
                    if let Ok(dir) = result {
                        if let Some(path) = dir.path() {
                            let path_str = path.to_string_lossy().to_string();
                            let mut all_paths = config.dictionary_paths();
                            if !all_paths.contains(&path_str) {
                                all_paths.push(path_str.clone());
                                config.set_dictionary_paths(&all_paths);
                                log::info!("Added dictionary directory: {}", path_str);
                                rebuild_path_rows(&path_rows_c, &config, &cb);
                            }
                        }
                    }
                });
            });
        }
        // Scan button in same group
        {
            let config = config.clone();
            let dm = dm.clone();
            let cb = on_dicts_changed.clone();
            let dict_rows_c = dict_rows.clone();
            let rfn_cell_c = rfn_cell.clone();
            scan_all_btn.connect_clicked(move |_| {
                let config = config.clone();
                let dm = dm.clone();
                let cb = cb.clone();
                let _dict_rows_c = dict_rows_c.clone();
                let rfn_cell_c = rfn_cell_c.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let dirs: Vec<std::path::PathBuf> = config.dictionary_paths().iter().map(std::path::PathBuf::from).collect();
                if dirs.is_empty() {
                    // No directories: clear all scanned dicts
                    config.set_scanned_dicts(&[]);
                    if let Ok(mut mgr) = dm.try_borrow_mut() {
                        mgr.clear_all();
                        mgr.add_online_dict(Arc::new(crate::engine::dict_manager::BaiduDict));
                    }
                    cb();
                    rfn_cell_c.borrow().as_ref().map(|f| f());
                    return;
                }
                std::thread::spawn(move || {
                    let mgr = crate::engine::dict_manager::DictManager::new();
                    let result = mgr.scan_directories(&dirs);
                    let _ = tx.send(result);
                });
                let rfn_clone = rfn_cell_c.clone();
                glib::idle_add_local(move || {
                    match rx.try_recv() {
                        Ok(Ok(found)) => {
                            let count = found.len();
                            config.set_scanned_dicts(&found);
                            if let Ok(mut mgr) = dm.try_borrow_mut() {
                                mgr.add_online_dict(Arc::new(crate::engine::dict_manager::BaiduDict));
                                mgr.sync_from_cache(&found);
                                let dict_states = config.load_dict_active_states();
                                if !dict_states.is_empty() { mgr.import_active_states(&dict_states); }
                                config.save_dict_active_states(&mgr.export_active_states());
                            }
                            log::info!("Scanned {} dictionaries", count);
                            cb();
                            {
                                let r = rfn_clone.borrow();
                                if let Some(ref f) = *r { f(); }
                            }
                            glib::ControlFlow::Break
                        }
                        Ok(Err(e)) => { log::warn!("Scan error: {}", e); glib::ControlFlow::Break }
                        Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                        Err(_) => glib::ControlFlow::Break,
                    }
                });
            });
        }
        page.add(&path_group);

        // === Dictionaries ===
        let dict_group = adw::PreferencesGroup::new();
        dict_group.set_title("Dictionaries");
        dict_group.add(&dict_rows);
        page.add(&dict_group);

        // === Baidu Translate ===
        let baidu_group = adw::PreferencesGroup::new();
        baidu_group.set_title("Baidu Translate");
        baidu_group.set_description(Some("Configure Baidu LLM Text Translation API credentials."));

        let (current_appid, current_secret) = config.baidu_credentials();

        let appid_row = adw::EntryRow::new();
        appid_row.set_title("App ID");
        appid_row.set_text(&current_appid);
        appid_row.set_show_apply_button(true);
        let secret_row = adw::EntryRow::new();
        secret_row.set_title("API Key");
        secret_row.set_text(&current_secret);
        secret_row.set_show_apply_button(true);

        {
            let config = config.clone();
            let secret_row = secret_row.clone();
            appid_row.connect_apply(move |row| {
                config.set_baidu_credentials(&row.text().trim(), &secret_row.text().trim());
            });
        }
        {
            let config = config.clone();
            let appid_row = appid_row.clone();
            secret_row.connect_apply(move |row| {
                config.set_baidu_credentials(&appid_row.text().trim(), &row.text().trim());
            });
        }

        baidu_group.add(&appid_row);
        baidu_group.add(&secret_row);
        page.add(&baidu_group);

        // === Search ===
        let search_group = adw::PreferencesGroup::new();
        search_group.set_title("Search");

        let max_results_adj = gtk4::Adjustment::new(50.0, 10.0, 500.0, 10.0, 50.0, 0.0);
        let max_results_row = adw::SpinRow::new(Some(&max_results_adj), 10.0, 0);
        max_results_row.set_title("Max Results");
        max_results_row.set_value(config.max_results() as f64);
        let c1 = config.clone();
        max_results_row.connect_changed(move |row| {
            c1.set_max_results(row.value() as i32);
        });
        search_group.add(&max_results_row);

        let fuzzy_adj = gtk4::Adjustment::new(3.0, 1.0, 10.0, 1.0, 1.0, 0.0);
        let fuzzy_row = adw::SpinRow::new(Some(&fuzzy_adj), 1.0, 0);
        fuzzy_row.set_title("Fuzzy Threshold");
        fuzzy_row.set_value(config.fuzzy_threshold() as f64);
        let c2 = config.clone();
        fuzzy_row.connect_changed(move |row| {
            c2.set_fuzzy_threshold(row.value() as i32);
        });
        search_group.add(&fuzzy_row);
        page.add(&search_group);

        window.add(&page);

        let config = config.clone();
        let pref = Self { window, dict_rows: dict_rows.clone(), path_rows: path_rows.clone(), rebuild_fn: rfn_cell, config: config.clone() };
        {
            let rfn = pref.rebuild_fn.clone();
            let dm2 = dm.clone();
            let cb2 = on_dicts_changed.clone();
            let rows2 = dict_rows.clone();
            let paths2 = path_rows.clone();
            let cfg = config.clone();
            let rebuild_all: Rc<dyn Fn()> = Rc::new({
                let rfn2 = rfn.clone();
                let cb3 = cb2.clone();
                let dm_order = dm2.clone();
                let cfg2 = cfg.clone();
                let paths3 = paths2.clone();
                move || {
                    eprintln!("rebuild_all: calling rfn + cb");
                    if let Some(ref f) = *rfn2.borrow() { f(); }
                    cb3();
                    rebuild_path_rows(&paths3, &cfg2, &cb3);
                    if let Ok(mgr) = dm_order.try_borrow() {
                        cfg2.save_dict_order(&mgr.export_order());
                    }
                }
            });
            *rfn.borrow_mut() = Some(Box::new(move || {
                rebuild_dict_rows_impl(&rows2, &dm2, &cb2, &rebuild_all, &cfg);
            }));
        }
        pref.rebuild_dict_rows(&dm, &on_dicts_changed);
        rebuild_path_rows(&pref.path_rows, &pref.config, &on_dicts_changed);
        pref
    }

    /// 重建词典行
    fn rebuild_dict_rows(&self, dm: &Rc<RefCell<DictManager>>, cb: &Rc<dyn Fn()>) {
        let rebuild_all: Rc<dyn Fn()> = Rc::new({
            let rfn = self.rebuild_fn.clone();
            let cb2 = cb.clone();
            move || {
                eprintln!("rebuild_dict_rows ra() enter");
                if let Some(ref f) = *rfn.borrow() { eprintln!("  calling stored fn"); f(); } else { eprintln!("  stored fn is None!"); }
                cb2();
                eprintln!("rebuild_dict_rows ra() done");
            }
        });
        rebuild_dict_rows_impl(&self.dict_rows, dm, cb, &rebuild_all, &self.config);
    }

    pub fn widget(&self) -> &adw::PreferencesWindow {
        &self.window
    }

    /// 设置父窗口，使首选项窗口成为模态对话框
    pub fn set_parent(&self, parent: &gtk4::Window) {
        self.window.set_transient_for(Some(parent));
        self.window.set_modal(true);
    }

    pub fn present(&self) {
        self.window.present();
    }
}
