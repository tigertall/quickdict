use gtk4::prelude::*;
use std::rc::Rc;

use crate::engine::types::DictInfo;

/// 侧栏词典列表组件
pub struct Sidebar {
    container: gtk4::Box,
    list_box: gtk4::ListBox,
    stats_label: gtk4::Label,
    checkbuttons: std::cell::RefCell<Vec<gtk4::CheckButton>>,
    dict_infos: std::cell::RefCell<Vec<DictInfo>>,
    toggle_callback: std::cell::RefCell<Option<Rc<dyn Fn(usize, bool)>>>,
    baidu_switch: gtk4::Switch,
}

impl Sidebar {
    pub fn new() -> Self {
        let list_box = gtk4::ListBox::new();
        list_box.add_css_class("navigation-sidebar");
        list_box.set_selection_mode(gtk4::SelectionMode::None);

        let stats_label = gtk4::Label::new(None);
        stats_label.add_css_class("dim-label");
        stats_label.add_css_class("caption");
        stats_label.set_margin_start(12);
        stats_label.set_margin_end(12);
        stats_label.set_margin_top(6);
        stats_label.set_margin_bottom(6);
        stats_label.set_xalign(0.0);

        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.set_width_request(260);
        container.set_vexpand(true);

        // Sidebar 标题
        let title = gtk4::Label::new(Some("词典"));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        title.set_margin_start(12);
        title.set_margin_end(12);
        title.set_margin_top(12);
        title.set_margin_bottom(6);
        container.append(&title);

        let scrolled = gtk4::ScrolledWindow::new();
        let inner_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        inner_box.set_vexpand(true);
        inner_box.append(&list_box);
        scrolled.set_child(Some(&inner_box));
        scrolled.set_vexpand(true);
        container.append(&scrolled);

        // 百度翻译开关
        let baidu_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        baidu_row.set_margin_start(12);
        baidu_row.set_margin_end(12);
        baidu_row.set_margin_top(4);
        baidu_row.set_margin_bottom(4);
        let baidu_label = gtk4::Label::new(Some("Baidu Translate"));
        baidu_label.set_xalign(0.0);
        baidu_label.set_hexpand(true);
        let baidu_switch = gtk4::Switch::new();
        baidu_switch.set_active(false);
        baidu_switch.set_valign(gtk4::Align::Center);
        baidu_row.append(&baidu_label);
        baidu_row.append(&baidu_switch);
        container.append(&baidu_row);

        container.append(&stats_label);

        Self {
            container,
            list_box,
            stats_label,
            checkbuttons: std::cell::RefCell::new(Vec::new()),
            dict_infos: std::cell::RefCell::new(Vec::new()),
            toggle_callback: std::cell::RefCell::new(None),
            baidu_switch,
        }
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    /// 更新词典列表，自动重连 toggle 回调
    pub fn set_dictionaries(&self, dicts: &[DictInfo]) {
        while let Some(row) = self.list_box.first_child() {
            self.list_box.remove(&row);
        }
        self.checkbuttons.borrow_mut().clear();
        *self.dict_infos.borrow_mut() = dicts.to_vec();

        let cb_opt = self.toggle_callback.borrow().clone();
        for (i, info) in dicts.iter().enumerate() {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            row.set_margin_start(6); row.set_margin_end(6);
            row.set_margin_top(2); row.set_margin_bottom(2);

            let cb = gtk4::CheckButton::with_label(&info.name);
            cb.set_active(info.enabled);
            cb.set_focus_on_click(false);
            cb.set_hexpand(true);
            row.append(&cb);

            // Reconnect toggle if callback is set
            if let Some(ref f) = cb_opt {
                let f = f.clone();
                cb.connect_toggled(move |cb| { f(i, cb.is_active()); });
            }
            self.checkbuttons.borrow_mut().push(cb);
            self.list_box.append(&row);
        }
        self.update_stats();
    }

    pub fn update_stats(&self) {
        let total: u64 = self.dict_infos.borrow().iter().map(|d| d.word_count).sum();
        self.stats_label.set_text(&format!(
            "{} dict(s) | {} words",
            self.dict_infos.borrow().len(), total
        ));
    }

    /// 连接词典启用/禁用切换（存储后自动在 set_dictionaries 重连）
    pub fn connect_toggle<F: Fn(usize, bool) + 'static>(&self, f: F) {
        let f: Rc<dyn Fn(usize, bool)> = Rc::new(f);
        for (i, cb) in self.checkbuttons.borrow().iter().enumerate() {
            let f = f.clone();
            cb.connect_toggled(move |cb| { f(i, cb.is_active()); });
        }
        *self.toggle_callback.borrow_mut() = Some(f);
    }

    /// 百度翻译是否启用
    pub fn is_baidu_enabled(&self) -> bool {
        self.baidu_switch.is_active()
    }

    /// 设置百度翻译开关状态
    pub fn set_baidu_enabled(&self, enabled: bool) {
        self.baidu_switch.set_active(enabled);
    }

    /// 连接百度翻译开关
    pub fn connect_baidu_toggle<F: Fn(bool) + 'static>(&self, f: F) {
        let f = Rc::new(f);
        self.baidu_switch.connect_active_notify(move |sw| {
            f(sw.is_active());
        });
    }
}
