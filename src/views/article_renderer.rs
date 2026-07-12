use gtk4::prelude::*;

/// 文章渲染器（基于 GtkTextView）
pub struct ArticleRenderer {
    scrolled: gtk4::ScrolledWindow,
}

impl ArticleRenderer {
    pub fn new() -> Self {
        let buffer = gtk4::TextBuffer::new(None);
        let text_view = gtk4::TextView::with_buffer(&buffer);
        text_view.set_editable(false);
        text_view.set_cursor_visible(false);
        text_view.set_wrap_mode(gtk4::WrapMode::Word);
        text_view.set_monospace(true);
        text_view.set_vexpand(true);
        text_view.set_hexpand(true);
        text_view.set_left_margin(12);
        text_view.set_right_margin(12);
        text_view.set_top_margin(8);
        text_view.set_bottom_margin(8);

        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_child(Some(&text_view));
        scrolled.set_vexpand(true);

        Self { scrolled }
    }

    pub fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.scrolled
    }
}
