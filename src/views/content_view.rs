use gtk4::prelude::*;
use std::cell::RefCell;
use std::fmt::Write;
use std::rc::Rc;

use super::word_list::WordList;
use super::article_renderer::ArticleRenderer;
use crate::engine::types::ArticleData;

/// 单个词典的释义折叠区域
struct ArticleExpander {
    container: gtk4::Box,
}

/// Type alias for link click handler
pub type LinkHandler = Rc<dyn Fn(String)>;

impl ArticleExpander {
    fn new(article: &ArticleData, link_handler: Option<&LinkHandler>) -> Self {
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
        label.set_margin_start(20);
        label.set_margin_end(12);
        label.set_margin_top(6);
        label.set_margin_bottom(8);
        label.set_selectable(true);
        label.set_focusable(false);

        // Connect link activation if handler provided
        if let Some(handler) = link_handler {
            let h = handler.clone();
            label.connect_activate_link(move |_, url| {
                if let Some(word) = url.strip_prefix("entry://") {
                    h(word.to_string());
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        }

        if article.is_html {
            label.set_use_markup(true);
            let safe_text = sanitize_null_bytes(&article.raw_text);
            let markup = html_to_pango_markup(&safe_text);
            log_raw_to_pango(&article.dict_name, &safe_text, &markup);
            label.set_markup(&markup);
        } else {
            let safe_text = sanitize_null_bytes(&article.raw_text);
            eprintln!("\n=== [{}] ===\n--- PLAINTEXT ({}B) ---\n{}", article.dict_name, safe_text.len(), safe_text);
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
    link_handler: RefCell<Option<LinkHandler>>,
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
            link_handler: RefCell::new(None),
        }
    }

    pub fn set_link_handler(&self, handler: LinkHandler) {
        *self.link_handler.borrow_mut() = Some(handler);
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

        let handler = self.link_handler.borrow();
        for article in &articles {
            let expander = ArticleExpander::new(article, handler.as_ref());
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

fn log_raw_to_pango(dict: &str, raw: &str, pango: &str) {
    eprintln!(
        "\n=== [{}] ===\n--- RAW ({}B) ---\n{}\n--- PANGO ({}B) ---\n{}",
        dict, raw.len(), raw, pango.len(), pango,
    );
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
            let mut quote_char: Option<char> = None;

            loop {
                match chars.next() {
                    None => break,
                    Some('>') if quote_char.is_none() => break,
                    Some(ch) if ch.is_whitespace() && quote_char.is_none() => {
                        if !tag_name.is_empty() {
                            in_attr = true;
                            attrs.push(ch);
                        }
                    }
                    Some(ch) => {
                        if !in_attr {
                            tag_name.push(ch.to_ascii_lowercase());
                        } else {
                            // Track quote state for attribute values
                            if let Some(q) = quote_char {
                                attrs.push(ch);
                                if ch == '\\' {
                                    if let Some(&next) = chars.peek() {
                                        attrs.push(next);
                                        chars.next();
                                    }
                                } else if ch == q {
                                    quote_char = None;
                                }
                            } else if ch == '"' || ch == '\'' {
                                quote_char = Some(ch);
                                attrs.push(ch);
                            } else {
                                attrs.push(ch);
                            }
                        }
                    }
                }
            }

            // Skip script/style content entirely
            if tag_name == "script" || tag_name == "style" {
                if !closing {
                    let end_tag = format!("</{}>", tag_name);
                    skip_until_close(&mut chars, &end_tag);
                }
                continue;
            }

            match tag_name.as_str() {
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
                        if !result.ends_with('\n') { result.push('\n'); }
                        for _ in 0..=list_level { result.push_str("  "); }
                        result.push('•');
                        result.push(' ');
                        skip_ws = true;
                    }
                }
                "ul" | "ol" => {
                    if closing { list_level = list_level.saturating_sub(1); }
                    else { list_level += 1; }
                }
                "font" | "span" => {
                    if !closing {
                        let has_block = is_style_block(&attrs) || is_class_block(&attrs);
                        if has_block && !result.ends_with('\n') && !result.is_empty() {
                            result.push('\n');
                        }
                        let c = extract_attr_value(&attrs, "color")
                            .or_else(|| {
                                if tag_name == "span" { extract_style_color(&attrs) } else { None }
                            });
                        if has_block {
                            // Push sentinel BEFORE color tag so close drains correctly
                            tag_stack.push("_block".into());
                        }
                        if let Some(c) = c {
                            let c = normalize_pango_color(&c);
                            push_open_tag("span", Some(("foreground", &escape_pango_attr(&c))), &mut result, &mut tag_stack);
                        }
                    } else {
                        // Close block spans first, inserting trailing newline
                        if let Some(pos) = tag_stack.iter().rposition(|t| t == "_block") {
                            // Close everything up to and including the block sentinel
                            for t in tag_stack.drain(pos..).rev() {
                                if t != "_block" {
                                    write!(result, "</{}>", t).unwrap();
                                }
                            }
                            if !result.ends_with('\n') { result.push('\n'); }
                            skip_ws = true;
                        } else {
                            close_tags_until(&mut result, &mut tag_stack, "span");
                        }
                    }
                }
                "a" => {
                    if !closing {
                        if let Some(h) = extract_attr_value(&attrs, "href") {
                            push_open_tag("a", Some(("href", &escape_pango_attr(&h))), &mut result, &mut tag_stack);
                        }
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, "a");
                    }
                }
                tag if let Some(pango) = passthrough_tag(tag) => {
                    if !closing {
                        if pango == "b" { try_insert_oxford_newline(&mut chars, &mut result); }
                        push_open_tag(pango, None, &mut result, &mut tag_stack);
                    } else {
                        close_tags_until(&mut result, &mut tag_stack, pango);
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
                // HTML entity names only contain [a-zA-Z0-9]; # only as first char
                if !ec.is_ascii_alphanumeric() && ec != '#' {
                    break;
                }
                if ec == '#' && !entity.is_empty() {
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
                    "nbsp" => result.push('\u{00A0}'),  // Unicode NBSP — Pango collapses regular spaces
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
                // Entity without semicolon — handle known ones
                match entity.as_str() {
                    "nbsp" => result.push('\u{00A0}'),
                    _ => {
                        result.push_str("&amp;");
                        result.push_str(&entity);
                    }
                }
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
                '\r' => {},  // Strip CR — ClutterText may fail on \r
                _ => result.push(c),
            }
        }
    }

    // 关闭所有未闭合标签（跳过 _block sentinel）
    for tag in tag_stack.iter().rev() {
        if tag == "_block" { result.push('\n'); continue; }
        write!(result, "</{}>", tag).unwrap();
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
    // 去除首部换行（ClutterText 首行全空时可能不计算高度）
    while cleaned.starts_with('\n') {
        cleaned.remove(0);
    }
    cleaned
}

/// Skip characters until the closing tag `end_tag` (case-insensitive) is found.
fn skip_until_close<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>, end_tag: &str) {
    let end_lower = end_tag.to_lowercase();
    let end_bytes = end_lower.as_bytes();
    let mut buf: Vec<u8> = Vec::with_capacity(end_bytes.len());

    while let Some(c) = chars.next() {
        buf.push(c.to_ascii_lowercase() as u8);
        if buf.len() > end_bytes.len() {
            buf.remove(0);
        }
        if buf == end_bytes {
            return;
        }
    }
}

fn close_tags_until(result: &mut String, stack: &mut Vec<String>, tag: &str) {
    if let Some(pos) = stack.iter().rposition(|t| t == tag) {
        for t in stack.drain(pos..).rev() {
            write!(result, "</{}>", t).unwrap();
        }
    }
}

/// Map HTML tag to its Pango passthrough equivalent (same tag name in both)
fn passthrough_tag(html: &str) -> Option<&'static str> {
    Some(match html {
        "b" | "strong" => "b",
        "i" | "em" => "i",
        "u" => "u",
        "s" | "strike" | "del" => "s",
        "sub" => "sub",
        "sup" => "sup",
        "big" => "big",
        "small" => "small",
        "tt" | "code" | "pre" => "tt",
        _ => return None,
    })
}

/// Push an open tag, optionally with a single attribute
fn push_open_tag(
    tag: &str,
    attr: Option<(&str, &str)>,
    result: &mut String,
    stack: &mut Vec<String>,
) {
    if let Some((name, val)) = attr {
        write!(result, "<{} {}='{}'>", tag, name, val).unwrap();
    } else {
        write!(result, "<{}>", tag).unwrap();
    }
    stack.push(tag.into());
}

/// Oxford-style numbering: insert newline before `<b>1.</b>` in running text
fn try_insert_oxford_newline<I: Iterator<Item = char> + Clone>(
    chars: &mut std::iter::Peekable<I>,
    result: &mut String,
) {
    let mut peek = chars.clone();
    if let Some(d) = peek.next() {
        if d.is_ascii_digit() {
            if let Some(dot) = peek.next() {
                if dot == '.' && !result.ends_with('\n') && !result.is_empty() {
                    result.push('\n');
                }
            }
        }
    }
}

fn extract_attr_value(attrs: &str, name: &str) -> Option<String> {
    let attrs = attrs.trim();
    let attrs_lower = attrs.to_lowercase();
    let name_lower = name.to_lowercase();
    let name_bytes = name_lower.as_bytes();

    let mut search_from = 0;
    while let Some(pos) = attrs_lower[search_from..].find(&name_lower) {
        let abs_pos = search_from + pos;
        // Word-boundary check: preceding char must not be alphanumeric or '-'
        // (prevents "color" matching inside "background-color")
        if abs_pos > 0 {
            let prev = attrs_lower.as_bytes()[abs_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'-' {
                search_from = abs_pos + name_bytes.len();
                continue;
            }
        }
        // Check that the character after name is whitespace or '='
        let after_name = &attrs[abs_pos + name_bytes.len()..];
        let first_after = after_name.chars().next()?;
        if !first_after.is_whitespace() && first_after != '=' {
            search_from = abs_pos + name_bytes.len();
            continue;
        }

        let after_eq = after_name.trim_start_matches(|c: char| c.is_whitespace());
        if !after_eq.starts_with('=') {
            search_from = abs_pos + name_bytes.len();
            continue;
        }
        let after = after_eq[1..].trim_start_matches(|c: char| c.is_whitespace());

        if after.is_empty() {
            return None;
        }

        let first_char = after.chars().next()?;
        if first_char == '"' || first_char == '\'' {
            // Quoted value — skip escaped quotes within
            let mut end = 0;
            let bytes = after.as_bytes();
            let mut i = 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2; // skip escaped char
                    continue;
                }
                if bytes[i] == first_char as u8 {
                    end = i;
                    break;
                }
                i += 1;
            }
            if end > 0 {
                return Some(after[1..end].to_string());
            }
            // Unclosed quote — fall through to best-effort
            if let Some(e) = after[1..].find(first_char) {
                return Some(after[1..=e].to_string());
            }
        } else {
            let end = after.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(after.len());
            if end > 0 {
                return Some(after[..end].to_string());
            }
        }
        return None;
    }
    None
}

fn extract_style_color(attrs: &str) -> Option<String> {
    let attrs_lower = attrs.to_lowercase();
    let mut search_start = 0;
    while let Some(pos) = attrs_lower[search_start..].find("color:") {
        let abs_pos = search_start + pos;
        if abs_pos == 0 || {
            let prev = attrs_lower.as_bytes()[abs_pos - 1];
            !prev.is_ascii_alphanumeric() && prev != b'-'
        } {
            let after = &attrs[abs_pos + 6..].trim_start();
            let end = after.find(|c: char| c == ';' || c == '"' || c == '\'').unwrap_or(after.len());
            return Some(after[..end].trim().to_string());
        }
        search_start = abs_pos + 1;
    }
    search_start = 0;
    while let Some(pos) = attrs_lower[search_start..].find("color=") {
        let abs_pos = search_start + pos;
        if abs_pos == 0 || {
            let prev = attrs_lower.as_bytes()[abs_pos - 1];
            !prev.is_ascii_alphanumeric() && prev != b'-'
        } {
            let after = &attrs[abs_pos + 6..].trim_start();
            let delim = after.chars().next()?;
            if delim == '"' || delim == '\'' {
                if let Some(end) = after[1..].find(delim) {
                    return Some(after[1..=end].to_string());
                }
            }
        }
        search_start = abs_pos + 1;
    }
    None
}

/// Check if style contains `display:block` or `display:inline-block`
fn is_style_block(attrs: &str) -> bool {
    let attrs_lower = attrs.to_lowercase();
    let mut search_start = 0;
    while let Some(pos) = attrs_lower[search_start..].find("display:") {
        let abs_pos = search_start + pos;
        if abs_pos == 0 || {
            let prev = attrs_lower.as_bytes()[abs_pos - 1];
            !prev.is_ascii_alphanumeric() && prev != b'-'
        } {
            let after = &attrs_lower[abs_pos + 8..].trim_start();
            if after.starts_with("block") || after.starts_with("inline-block") {
                return true;
            }
        }
        search_start = abs_pos + 1;
    }
    false
}

/// Known dictionary class names that act as block-level sections.
/// Matches common MDX dictionary CSS class conventions (sf_ecce.css style).
const BLOCK_CLASSES: &[&str] = &[
    "trs",   // translation/part-of-speech section
    "syno",  // synonyms section
    "phrs",  // phrases section
    "phr",   // individual phrase entry
    "wfs",   // word forms section
    "wf",    // individual word form entry
    "exam",  // example sentence section
];

/// Check if class attribute contains a known block-level dictionary class name
fn is_class_block(attrs: &str) -> bool {
    let class_val = match extract_attr_value(attrs, "class") {
        Some(v) => v,
        None => return false,
    };
    class_val.split_whitespace().any(|c| BLOCK_CLASSES.contains(&c))
}

fn escape_pango_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('\'', "&#39;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Normalize CSS color to Pango-compatible format.
/// Pango rejects rgb() values with leading zeros (e.g. rgb(098,100,038));
/// convert them to #RRGGBB hex which Pango reliably accepts.
fn normalize_pango_color(color: &str) -> String {
    let lower = color.trim().to_lowercase();
    // rgb(r,g,b) → #RRGGBB
    if let Some(rest) = lower.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = rest.split(',').collect();
        if parts.len() == 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].trim().parse::<u8>(),
                parts[1].trim().parse::<u8>(),
                parts[2].trim().parse::<u8>(),
            ) {
                return format!("#{:02X}{:02X}{:02X}", r, g, b);
            }
        }
    }
    // rgba(r,g,b,a) → #RRGGBB (discard alpha — Pango simple markup has no alpha)
    if let Some(rest) = lower.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = rest.split(',').collect();
        if parts.len() >= 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].trim().parse::<u8>(),
                parts[1].trim().parse::<u8>(),
                parts[2].trim().parse::<u8>(),
            ) {
                return format!("#{:02X}{:02X}{:02X}", r, g, b);
            }
        }
    }
    color.to_string()
}

/// Remove interior null bytes that would crash GLib/Pango
fn sanitize_null_bytes(s: &str) -> String {
    if s.contains('\0') {
        s.chars().filter(|&c| c != '\0').collect()
    } else {
        s.to_string()
    }
}
