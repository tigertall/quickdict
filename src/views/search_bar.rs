use gtk4::prelude::*;
use std::rc::Rc;

/// 搜索栏组件
pub struct SearchBar {
    pub entry: gtk4::SearchEntry,
    pub search_btn: gtk4::Button,
    container_: gtk4::Box,
}

impl SearchBar {
    pub fn new() -> Self {
        let entry = gtk4::SearchEntry::new();
        entry.set_placeholder_text(Some("Search dictionary..."));
        entry.set_width_chars(30);

        let search_btn = gtk4::Button::from_icon_name("system-search-symbolic");
        search_btn.add_css_class("flat");

        let container_ = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        container_.append(&entry);
        container_.append(&search_btn);

        Self {
            entry,
            search_btn,
            container_,
        }
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container_
    }

    pub fn connect_search<F: Fn(&str) + 'static>(&self, f: F) {
        let f = Rc::new(f);
        let entry = self.entry.clone();
        let f1 = f.clone();
        self.entry.connect_activate(move |e| {
            f1(&e.text());
        });
        let f2 = f.clone();
        self.search_btn.connect_clicked(move |_| {
            f2(&entry.text());
        });
    }
}
