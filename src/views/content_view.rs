use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::word_list::WordList;
use super::article_renderer::ArticleRenderer;
use crate::engine::types::ArticleData;

/// 单个词典的释义折叠区域
struct ArticleExpander {
    container: gtk4::Box,
}

impl ArticleExpander {
    fn new(article: &ArticleData) -> Self {
        // 词典名称标签（自定义样式）
        let safe_name = sanitize_null_bytes(&article.dict_name);
        let title_label = gtk4::Label::new(Some(&safe_name));
        title_label.set_use_markup(true);
        title_label.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(&safe_name)));
        title_label.set_xalign(0.0);
        title_label.set_margin_start(6);

        let expander = gtk4::Expander::new(None);
        expander.set_label_widget(Some(&title_label));
        expander.set_expanded(true);
        expander.set_margin_start(12);
        expander.set_margin_end(8);
        expander.set_margin_top(6);
        expander.set_margin_bottom(6);

        let label = gtk4::Label::new(None);
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.set_selectable(true);
        label.set_margin_start(20);
        label.set_margin_end(12);
        label.set_margin_top(6);
        label.set_margin_bottom(8);

        if article.is_html {
            label.set_use_markup(true);
            let safe_text = sanitize_null_bytes(&article.raw_text);
            let markup = html_to_pango_markup(&safe_text);
            label.set_markup(&markup);
        } else {
            let safe_text = sanitize_null_bytes(&article.raw_text);
            label.set_label(&safe_text);
        }

        expander.set_child(Some(&label));

        // 外层容器：左侧彩色竖线 + 背景
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.add_css_class("dict-container");
        container.append(&expander);

        Self {
            container,
        }
    }

    fn widget(&self) -> &gtk4::Box {
        &self.container
    }
}

/// 主内容区组件（Stack 布局）
pub struct ContentView {
    stack: gtk4::Stack,
    word_list: WordList,

    updating: std::cell::Cell<bool>,
    /// 多词典文章容器
    articles_box: gtk4::Box,

    expanders: Rc<RefCell<Vec<ArticleExpander>>>,
}

impl ContentView {
    pub fn new() -> Self {
        let word_list = WordList::new();
        let renderer = ArticleRenderer::new();

        let waiting_label = gtk4::Label::new(Some("Enter a word and press Enter to search"));
        waiting_label.add_css_class("dim-label");
        waiting_label.add_css_class("title-4");

        let waiting_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        waiting_box.set_valign(gtk4::Align::Center);
        waiting_box.set_halign(gtk4::Align::Center);
        waiting_box.set_vexpand(true);
        waiting_box.append(&waiting_label);

        let scrolled_list = gtk4::ScrolledWindow::new();
        scrolled_list.set_child(Some(word_list.widget()));

        // 多词典文章视图
        let articles_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        articles_box.set_valign(gtk4::Align::Start);

        let articles_scrolled = gtk4::ScrolledWindow::new();
        articles_scrolled.set_child(Some(&articles_box));
        articles_scrolled.set_vexpand(true);

        let stack = gtk4::Stack::new();
        stack.add_named(&waiting_box, Some("waiting"));
        stack.add_named(&scrolled_list, Some("results"));
        stack.add_named(renderer.widget(), Some("article"));
        stack.add_named(&articles_scrolled, Some("articles"));
        stack.set_visible_child_name("waiting");

        // 加载词典释义样式
        let css = gtk4::CssProvider::new();
        css.load_from_string(
            ".dict-container { \
               border-left: 4px solid @accent_bg_color; \
               border-radius: 8px; \
               background: alpha(@accent_bg_color, 0.06); \
               margin: 6px 8px; \
             }"
        );
        gtk4::style_context_add_provider_for_display(
            &gdk4::Display::default().expect("No display"),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        Self {
            stack,
            word_list,
            updating: std::cell::Cell::new(false),
            articles_box,
            expanders: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn widget(&self) -> &gtk4::Stack {
        &self.stack
    }

    pub fn word_list(&self) -> &WordList {
        &self.word_list
    }

    pub fn begin_update(&self) {
        self.updating.set(true);
    }

    pub fn end_update(&self) {
        self.updating.set(false);
    }

    pub fn is_updating(&self) -> bool {
        self.updating.get()
    }

    /// 切换状态


    /// 显示多词典文章（带折叠功能）
    pub fn show_multi_articles(&self, articles: Vec<ArticleData>) {
        // 清除旧内容
        while let Some(child) = self.articles_box.first_child() {
            self.articles_box.remove(&child);
        }
        self.expanders.borrow_mut().clear();

        for article in &articles {
            let expander = ArticleExpander::new(article);
            self.articles_box.append(expander.widget());
            self.expanders.borrow_mut().push(expander);
        }

        self.stack.set_visible_child_name("articles");
    }

    /// 显示单个词典文章（兼容旧接口）
    pub fn show_single_article(&self, article: &ArticleData) {
        self.show_multi_articles(vec![article.clone()]);
    }

    /// 显示搜索结果列表
    pub fn show_results(&self, results: &[crate::engine::types::SearchResult]) {
        self.begin_update();
        self.word_list.set_results(results);
        self.end_update();
        self.stack.set_visible_child_name("results");
    }
}

/// HTML → Pango markup (保留 color/font/b/i/u/sub/sup/tt 等格式)
pub(crate) fn html_to_pango_markup(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    let mut tag_stack: Vec<String> = Vec::new();
    let mut list_level = 0u32;
    let mut skip_ws = false;

    while let Some(c) = chars.next() {
        if c == '<' {
            let closing = chars.peek() == Some(&'/');
            if closing {
                chars.next();
            }

            let mut tag_name = String::new();
            let mut attrs = String::new();
            let mut in_attr = false;

            loop {
                match chars.next() {
                    None => break,
                    Some('>') => break,
                    Some(ch) if ch.is_whitespace() => {
                        if !tag_name.is_empty() {
                            in_attr = true;
                        }
                    }
                    Some(ch) if !in_attr => {
                        tag_name.push(ch.to_ascii_lowercase());
                    }
                    Some(ch) => {
                        attrs.push(ch);
                    }
                }
            }

            match tag_name.as_str() {
                // 块级元素 → 换行（跳过紧跟 li 后的空白场景）
                "br" => { result.push('\n'); skip_ws = true; }
                "p" | "div" | "tr" | "hr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "dd" | "dt" => {
                    if !closing {
                        if !skip_ws && !result.ends_with('\n') && !result.is_empty() { result.push('\n'); }
                    } else if !result.ends_with('\n') {
                        result.push('\n');
                        skip_ws = true;
                    }
                }
                "li" => {
                    if !closing {
                        if !result.ends_with('\n') {
                            result.push('\n');
                        }
                        for _ in 0..=list_level {
                            result.push_str("  ");
                        }
                        result.push('•');
                        result.push(' ');
                        skip_ws = true;
                    }
                }
                "ul" | "ol" => {
                    if closing {
                        list_level = list_level.saturating_sub(1);
                    } else {
                        list_level += 1;
                    }
                }
                // 保留的格式标签（Pango 相同）
                "b" | "strong" => {
                    if !closing {
                        result.push_str("<b>");
                        tag_stack.push("b".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "b");
                    }
                }
                "i" | "em" => {
                    if !closing {
                        result.push_str("<i>");
                        tag_stack.push("i".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "i");
                    }
                }
                "u" => {
                    if !closing {
                        result.push_str("<u>");
                        tag_stack.push("u".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "u");
                    }
                }
                "s" | "strike" | "del" => {
                    if !closing {
                        result.push_str("<s>");
                        tag_stack.push("s".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "s");
                    }
                }
                "sub" => {
                    if !closing {
                        result.push_str("<sub>");
                        tag_stack.push("sub".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "sub");
                    }
                }
                "sup" => {
                    if !closing {
                        result.push_str("<sup>");
                        tag_stack.push("sup".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "sup");
                    }
                }
                "big" => {
                    if !closing {
                        result.push_str("<big>");
                        tag_stack.push("big".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "big");
                    }
                }
                "small" => {
                    if !closing {
                        result.push_str("<small>");
                        tag_stack.push("small".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "small");
                    }
                }
                "tt" | "code" | "pre" => {
                    if !closing {
                        result.push_str("<tt>");
                        tag_stack.push("tt".into());
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "tt");
                    }
                }
                // font 标签 → span
                "font" => {
                    if !closing {
                        let color = extract_attr_value(&attrs, "color");
                        if let Some(c) = color {
                            result.push_str(&format!("<span foreground='{}'>", escape_pango_attr(&c)));
                            tag_stack.push("span".into());
                        }
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "span");
                    }
                }
                // span 标签 → span（保留 color/style）
                "span" => {
                    if !closing {
                        let color = extract_attr_value(&attrs, "color");
                        let style_color = extract_style_color(&attrs);
                        let c = color.or(style_color);
                        if let Some(c) = c {
                            result.push_str(&format!("<span foreground='{}'>", escape_pango_attr(&c)));
                            tag_stack.push("span".into());
                        }
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "span");
                    }
                }
                // a 标签
                "a" => {
                    if !closing {
                        let href = extract_attr_value(&attrs, "href");
                        if let Some(h) = href {
                            result.push_str(&format!("<a href='{}'>", escape_pango_attr(&h)));
                            tag_stack.push("a".into());
                        }
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "a");
                    }
                }
                _ => {}
            }
        } else if c == '&' {
            // HTML 实体解码
            let mut entity = String::new();
            let mut had_semicolon = false;
            while let Some(&ec) = chars.peek() {
                if ec == ';' {
                    had_semicolon = true;
                    chars.next();
                    break;
                }
                if ec == ' ' || ec == '<' || ec == '>' {
                    break;
                }
                entity.push(chars.next().unwrap());
            }
            if had_semicolon {
                match entity.as_str() {
                    "amp" => result.push_str("&amp;"),
                    "lt" => result.push_str("&lt;"),
                    "gt" => result.push_str("&gt;"),
                    "quot" => result.push_str("&quot;"),
                    "apos" => result.push_str("&#39;"),
                    "nbsp" => result.push(' '),
                    _ if entity.starts_with('#') && entity.len() > 1 => {
                        let num_str = if entity.as_bytes()[1] == b'x' {
                            &entity[2..]
                        } else {
                            &entity[1..]
                        };
                        if let Ok(code) = u32::from_str_radix(num_str, if entity.as_bytes()[1] == b'x' { 16 } else { 10 }) {
                            if let Some(ch) = char::from_u32(code) {
                                result.push(ch);
                            }
                        }
                    }
                    _ => {
                        result.push_str("&amp;");
                        result.push_str(&entity);
                        result.push(';');
                    }
                }
            } else {
                result.push_str("&amp;");
                result.push_str(&entity);
            }
        } else {
            // 纯文本：转义 Pango 特殊字符
            if skip_ws {
                if c == '\n' || c == '\r' || c == ' ' || c == '\t' {
                    continue;
                }
                skip_ws = false;
            }
            match c {
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '&' => result.push_str("&amp;"),
                _ => result.push(c),
            }
        }
    }

    // 关闭所有未闭合标签
    for tag in tag_stack.iter().rev() {
        result.push_str("</");
        result.push_str(tag);
        result.push('>');
    }

    // 压缩多余空行（3+ → 2）
    let mut cleaned = String::with_capacity(result.len());
    let mut newlines = 0u32;
    for ch in result.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines <= 2 {
                cleaned.push('\n');
            }
        } else {
            newlines = 0;
            cleaned.push(ch);
        }
    }
    // 去除尾部多余换行
    while cleaned.ends_with('\n') {
        cleaned.pop();
    }
    cleaned
}

fn close_tags_until(result: &mut String, stack: &mut Vec<String>, tag: &str) {
    // 查找并关闭到指定标签
    if let Some(pos) = stack.iter().rposition(|t| t == tag) {
        for t in stack.drain(pos..).rev() {
            result.push_str("</");
            result.push_str(&t);
            result.push('>');
        }
    }
}

fn extract_attr_value(attrs: &str, name: &str) -> Option<String> {
    // 规范化：去除首尾空白
    let attrs = attrs.trim();
    // 在原始字符串中查找属性名（大小写不敏感）
    let attrs_lower = attrs.to_lowercase();
    let name_lower = name.to_lowercase();

    // 查找 name 后紧跟 = 的模式（允许 name 和 = 之间有空白）
    if let Some(pos) = attrs_lower.find(&name_lower) {
        let after_name = &attrs[pos + name_lower.len()..];
        // 跳过 name 和 = 之间的空白
        let after_eq = after_name.trim_start_matches(|c: char| c.is_whitespace());
        if !after_eq.starts_with('=') {
            return None;
        }
        // 跳过 =
        let after = after_eq[1..].trim_start_matches(|c: char| c.is_whitespace());

        if after.is_empty() {
            return None;
        }

        let first_char = after.chars().next()?;
        if first_char == '"' || first_char == '\'' {
            // 引号包裹的值
            if let Some(end) = after[1..].find(first_char) {
                return Some(after[1..=end].to_string());
            }
        } else {
            // 无引号的值，读到空白或结束
            let end = after.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(after.len());
            if end > 0 {
                return Some(after[..end].to_string());
            }
        }
    }
    None
}

fn extract_style_color(attrs: &str) -> Option<String> {
    let attrs_lower = attrs.to_lowercase();
    if let Some(pos) = attrs_lower.find("color:") {
        let after = &attrs[pos + 6..].trim_start();
        let end = after.find(|c: char| c == ';' || c == '"' || c == '\'').unwrap_or(after.len());
        return Some(after[..end].trim().to_string());
    }
    if let Some(pos) = attrs_lower.find("color=") {
        let after = &attrs[pos + 6..].trim_start();
        let delim = after.chars().next()?;
        if delim == '"' || delim == '\'' {
            if let Some(end) = after[1..].find(delim) {
                return Some(after[1..=end].to_string());
            }
        }
    }
    None
}

fn escape_pango_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('\'', "&#39;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Remove interior null bytes that would crash GLib/Pango
fn sanitize_null_bytes(s: &str) -> String {
    if s.contains('\0') {
        s.chars().filter(|&c| c != '\0').collect()
    } else {
        s.to_string()
    }
}
