use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::engine::types::SearchResult;

/// 搜索结果列表组件
pub struct WordList {
    pub list_view: gtk4::ListView,
    model: gtk4::StringList,
    selection: gtk4::SingleSelection,
    results: Rc<RefCell<Vec<SearchResult>>>,
}

impl WordList {
    pub fn new() -> Self {
        let model = gtk4::StringList::new(&[]);
        let selection = gtk4::SingleSelection::new(Some(model.clone()));
        selection.set_autoselect(false);

        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let label = gtk4::Label::new(None);
            label.set_xalign(0.0);
            label.set_margin_start(12);
            label.set_margin_end(12);
            label.set_margin_top(6);
            label.set_margin_bottom(6);
            if let Some(list_item) = item.downcast_ref::<gtk4::ListItem>() {
                list_item.set_child(Some(&label));
            }
        });

        factory.connect_bind(|_, item| {
            if let Some(list_item) = item.downcast_ref::<gtk4::ListItem>() {
                if let Some(string_obj) = list_item.item().and_downcast::<gtk4::StringObject>() {
                    if let Some(label) = list_item.child().and_downcast::<gtk4::Label>() {
                        label.set_text(&string_obj.string());
                    }
                }
            }
        });

        let list_view = gtk4::ListView::new(Some(selection.clone()), Some(factory));
        list_view.set_vexpand(true);

        Self {
            list_view,
            model,
            selection,
            results: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// 设置搜索结果
    pub fn set_results(&self, results: &[SearchResult]) {
        let strings: Vec<String> = results
            .iter()
            .map(|r| format!("{}  —  {}  ({:.0}%)", r.word, r.dict_name, r.score * 100.0))
            .collect();

        *self.results.borrow_mut() = results.to_vec();

        let n = self.model.n_items();
        if n > 0 {
            let refs: Vec<&str> = strings.iter().map(|s| s.as_str()).collect();
            self.model.splice(0, n, &refs);
        } else {
            for s in &strings {
                self.model.append(s.as_str());
            }
        }
    }

    /// 清空结果


    /// 连接选择变更
    pub fn connect_selected<F: Fn(usize) + 'static>(&self, f: F) {
        let selection = self.selection.clone();
        selection.connect_selection_changed(move |sel, _, _| {
            let pos = sel.selected();
            f(pos as usize);
        });
    }

    /// 获取搜索结果（供外部读取）
    pub fn results(&self) -> &Rc<RefCell<Vec<SearchResult>>> {
        &self.results
    }

    pub fn widget(&self) -> &gtk4::ListView {
        &self.list_view
    }
}
