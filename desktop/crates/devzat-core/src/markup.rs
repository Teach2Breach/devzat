//! Parses devzat message text (markdown that may carry ANSI colors) into a small document model
//! that a GUI can lay out.

use crate::ansi::{self, AnsiStyle, Token};
use pulldown_cmark::{CodeBlockKind, Event as Md, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Private-use code points stand in for ANSI sequences while markdown is parsed.
const PLACEHOLDER_BASE: u32 = 0xF0000;
const HQ_PREFIX: &str = "dzhq:";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub ansi: AnsiStyle,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inline {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Para(Vec<Inline>),
    Heading { level: u8, inlines: Vec<Inline> },
    Code { lang: String, inlines: Vec<Inline> },
    Quote(Vec<Block>),
    List { start: Option<u64>, items: Vec<Vec<Block>> },
    Image { alt: String, src: String, hq: bool },
    Table { rows: Vec<Vec<Vec<Inline>>>, header_rows: usize },
    Rule,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub blocks: Vec<Block>,
}

impl Doc {
    /// Images in reading order, as `(src, hq)`.
    pub fn images(&self) -> Vec<(String, bool)> {
        fn walk(blocks: &[Block], out: &mut Vec<(String, bool)>) {
            for b in blocks {
                match b {
                    Block::Image { src, hq, .. } => out.push((src.clone(), *hq)),
                    Block::Quote(inner) => walk(inner, out),
                    Block::List { items, .. } => items.iter().for_each(|i| walk(i, out)),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.blocks, &mut out);
        out
    }

    /// The text a person would want when copying the message.
    pub fn plain_text(&self) -> String {
        fn inl(v: &[Inline], out: &mut String) {
            v.iter().for_each(|i| out.push_str(&i.text));
        }
        fn walk(blocks: &[Block], out: &mut String) {
            for b in blocks {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                match b {
                    Block::Para(v) | Block::Heading { inlines: v, .. } => inl(v, out),
                    Block::Code { inlines, .. } => inl(inlines, out),
                    Block::Quote(inner) => walk(inner, out),
                    Block::List { items, .. } => items.iter().for_each(|i| walk(i, out)),
                    Block::Image { alt, src, .. } => out.push_str(if alt.is_empty() { src } else { alt }),
                    Block::Table { rows, .. } => {
                        for row in rows {
                            let cells: Vec<String> = row.iter().map(|c| c.iter().map(|i| i.text.as_str()).collect()).collect();
                            out.push_str(&cells.join(" | "));
                            out.push('\n');
                        }
                    }
                    Block::Rule => out.push_str("---"),
                }
            }
        }
        let mut out = String::new();
        walk(&self.blocks, &mut out);
        out.trim().to_string()
    }
}

pub fn parse(text: &str) -> Doc {
    let mut prepped = String::with_capacity(text.len());
    let mut sgr: Vec<Vec<u16>> = Vec::new();
    for token in ansi::tokens(text) {
        match token {
            Token::Text(t) => prepped.push_str(t),
            Token::Sgr(params) => {
                if let Some(c) = char::from_u32(PLACEHOLDER_BASE + sgr.len() as u32) {
                    prepped.push(c);
                    sgr.push(params);
                }
            }
        }
    }
    let prepped = rewrite_hq(&prepped);
    let mut b = Builder::new(&sgr);
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    for ev in Parser::new_ext(&prepped, opts) {
        b.event(ev);
    }
    b.finish()
}

/// `!hq[alt](src)`, `![hq](src)` and `![hq: alt](src)` mark one picture to be shown sharp.
fn rewrite_hq(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    loop {
        let a = rest.find("!hq[");
        let b = rest.find("![hq");
        let next = match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        };
        let Some(i) = next else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let (alt_start, is_alt_form) = if tail.starts_with("!hq[") { (4, false) } else { (2, true) };
        if let Some(close) = tail[alt_start..].find("](") {
            let alt_raw = &tail[alt_start..alt_start + close];
            let alt = if is_alt_form {
                let t = alt_raw.strip_prefix("hq");
                match t {
                    Some("") => Some(""),
                    Some(t) if t.starts_with(':') => Some(t[1..].trim()),
                    _ => None,
                }
            } else {
                Some(alt_raw)
            };
            let src_start = alt_start + close + 2;
            if let (Some(alt), Some(end)) = (alt, tail[src_start..].find(')')) {
                let src = tail[src_start..src_start + end].trim();
                if !src.is_empty() && !src.contains(char::is_whitespace) {
                    out.push_str("![");
                    out.push_str(alt);
                    out.push_str("](");
                    if !src.starts_with(HQ_PREFIX) {
                        out.push_str(HQ_PREFIX);
                    }
                    out.push_str(src);
                    out.push(')');
                    rest = &tail[src_start + end + 1..];
                    continue;
                }
            }
        }
        out.push_str(&tail[..alt_start]);
        rest = &tail[alt_start..];
    }
}

enum Frame {
    Root(Vec<Block>),
    Quote(Vec<Block>),
    List { start: Option<u64>, items: Vec<Vec<Block>> },
    Item(Vec<Block>),
    Table { rows: Vec<Vec<Vec<Inline>>>, row: Vec<Vec<Inline>>, header_rows: usize },
}

impl Frame {
    fn blocks(&mut self) -> Option<&mut Vec<Block>> {
        match self {
            Frame::Root(b) | Frame::Quote(b) | Frame::Item(b) => Some(b),
            _ => None,
        }
    }
}

struct Builder<'s> {
    sgr: &'s [Vec<u16>],
    ansi: AnsiStyle,
    frames: Vec<Frame>,
    inlines: Vec<Inline>,
    bold: u32,
    italic: u32,
    strike: u32,
    links: Vec<String>,
    heading: Option<u8>,
    code: Option<(String, String)>,
    image: Option<(String, String)>,
}

impl<'s> Builder<'s> {
    fn new(sgr: &'s [Vec<u16>]) -> Self {
        Builder {
            sgr,
            ansi: AnsiStyle::default(),
            frames: vec![Frame::Root(Vec::new())],
            inlines: Vec::new(),
            bold: 0,
            italic: 0,
            strike: 0,
            links: Vec::new(),
            heading: None,
            code: None,
            image: None,
        }
    }

    fn style(&self, code: bool) -> Style {
        Style {
            ansi: self.ansi,
            bold: self.bold > 0 || self.heading.is_some(),
            italic: self.italic > 0,
            strike: self.strike > 0,
            code,
            link: self.links.last().cloned(),
        }
    }

    /// Appends text, applying any ANSI placeholders it contains.
    fn push_text(&mut self, text: &str, code: bool) {
        let mut run = String::new();
        for c in text.chars() {
            let n = c as u32;
            if n >= PLACEHOLDER_BASE && ((n - PLACEHOLDER_BASE) as usize) < self.sgr.len() {
                if !run.is_empty() {
                    let style = self.style(code);
                    push_inline(&mut self.inlines, std::mem::take(&mut run), style);
                }
                let params = &self.sgr[(n - PLACEHOLDER_BASE) as usize];
                ansi::apply_sgr(&mut self.ansi, params);
            } else {
                run.push(c);
            }
        }
        if !run.is_empty() {
            let style = self.style(code);
            push_inline(&mut self.inlines, run, style);
        }
    }

    fn top_blocks(&mut self) -> &mut Vec<Block> {
        if self.frames.last_mut().and_then(Frame::blocks).is_none() {
            self.frames.push(Frame::Item(Vec::new()));
        }
        self.frames.last_mut().and_then(Frame::blocks).expect("block frame")
    }

    fn flush_para(&mut self) {
        if self.inlines.iter().all(|i| i.text.trim().is_empty()) {
            self.inlines.clear();
            return;
        }
        let inlines = linkify(trim_inlines(std::mem::take(&mut self.inlines)));
        let block = match self.heading {
            Some(level) => Block::Heading { level, inlines },
            None => Block::Para(inlines),
        };
        self.top_blocks().push(block);
    }

    fn event(&mut self, ev: Md) {
        if let Some((_, buf)) = self.code.as_mut() {
            match ev {
                Md::Text(t) => {
                    buf.push_str(&t);
                    return;
                }
                Md::End(TagEnd::CodeBlock) => {}
                _ => return,
            }
        }
        if let Some((_, alt)) = self.image.as_mut() {
            match ev {
                Md::Text(t) | Md::Code(t) => {
                    alt.push_str(&ansi_free(&t));
                    return;
                }
                Md::End(TagEnd::Image) => {}
                _ => return,
            }
        }
        match ev {
            Md::Start(tag) => self.start(tag),
            Md::End(tag) => self.end(tag),
            Md::Text(t) => {
                let t = replace_shortcodes(&t);
                self.push_text(&t, false);
            }
            Md::Code(t) => self.push_text(&t, true),
            Md::Html(t) | Md::InlineHtml(t) => self.push_text(&t, false),
            Md::SoftBreak | Md::HardBreak => self.push_text("\n", false),
            Md::Rule => {
                self.flush_para();
                self.top_blocks().push(Block::Rule);
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush_para();
                self.heading = Some(heading_level(level));
            }
            Tag::BlockQuote(_) => {
                self.flush_para();
                self.frames.push(Frame::Quote(Vec::new()));
            }
            Tag::CodeBlock(kind) => {
                self.flush_para();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::List(start) => {
                self.flush_para();
                self.frames.push(Frame::List { start, items: Vec::new() });
            }
            Tag::Item => {
                self.flush_para();
                self.frames.push(Frame::Item(Vec::new()));
            }
            Tag::Table(_) => {
                self.flush_para();
                self.frames.push(Frame::Table { rows: Vec::new(), row: Vec::new(), header_rows: 0 });
            }
            Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { dest_url, .. } => self.links.push(dest_url.to_string()),
            Tag::Image { dest_url, .. } => {
                self.flush_para();
                self.image = Some((dest_url.to_string(), String::new()));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush_para(),
            TagEnd::Heading(_) => {
                self.flush_para();
                self.heading = None;
            }
            TagEnd::BlockQuote(_) => {
                self.flush_para();
                if let Some(Frame::Quote(blocks)) = self.frames.pop() {
                    self.top_blocks().push(Block::Quote(blocks));
                }
            }
            TagEnd::CodeBlock => {
                if let Some((lang, buf)) = self.code.take() {
                    let saved = std::mem::take(&mut self.inlines);
                    self.push_text(buf.trim_end_matches('\n'), true);
                    let inlines = std::mem::replace(&mut self.inlines, saved);
                    self.top_blocks().push(Block::Code { lang, inlines });
                }
            }
            TagEnd::Item => {
                self.flush_para();
                if let Some(Frame::Item(blocks)) = self.frames.pop() {
                    if let Some(Frame::List { items, .. }) = self.frames.last_mut() {
                        items.push(blocks);
                    } else {
                        self.top_blocks().extend(blocks);
                    }
                }
            }
            TagEnd::List(_) => {
                self.flush_para();
                if let Some(Frame::List { start, items }) = self.frames.pop() {
                    self.top_blocks().push(Block::List { start, items });
                }
            }
            TagEnd::TableCell => {
                let cell = trim_inlines(std::mem::take(&mut self.inlines));
                if let Some(Frame::Table { row, .. }) = self.frames.last_mut() {
                    row.push(cell);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(Frame::Table { rows, row, header_rows }) = self.frames.last_mut() {
                    rows.push(std::mem::take(row));
                    if matches!(tag, TagEnd::TableHead) {
                        *header_rows += 1;
                    }
                }
            }
            TagEnd::Table => {
                if let Some(Frame::Table { rows, header_rows, .. }) = self.frames.pop() {
                    self.top_blocks().push(Block::Table { rows, header_rows });
                }
            }
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => {
                self.links.pop();
            }
            TagEnd::Image => {
                if let Some((src, alt)) = self.image.take() {
                    let (src, hq) = match src.strip_prefix(HQ_PREFIX) {
                        Some(s) => (s.to_string(), true),
                        None => (src, false),
                    };
                    self.top_blocks().push(Block::Image { alt: alt.trim().to_string(), src, hq });
                }
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Doc {
        self.flush_para();
        while self.frames.len() > 1 {
            let frame = self.frames.pop().expect("frame");
            let blocks = match frame {
                Frame::Quote(b) => vec![Block::Quote(b)],
                Frame::Item(b) => b,
                Frame::List { start, items } => vec![Block::List { start, items }],
                Frame::Table { rows, header_rows, .. } => vec![Block::Table { rows, header_rows }],
                Frame::Root(b) => b,
            };
            self.top_blocks().extend(blocks);
        }
        match self.frames.pop() {
            Some(Frame::Root(blocks)) => Doc { blocks },
            _ => Doc::default(),
        }
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn push_inline(inlines: &mut Vec<Inline>, text: String, style: Style) {
    match inlines.last_mut() {
        Some(last) if last.style == style => last.text.push_str(&text),
        _ => inlines.push(Inline { text, style }),
    }
}

fn ansi_free(s: &str) -> String {
    s.chars()
        .filter(|&c| (c as u32) < PLACEHOLDER_BASE)
        .collect()
}

fn trim_inlines(mut v: Vec<Inline>) -> Vec<Inline> {
    while v.first().is_some_and(|i| i.text.trim().is_empty() && !i.style.code) {
        v.remove(0);
    }
    while v.last().is_some_and(|i| i.text.trim().is_empty() && !i.style.code) {
        v.pop();
    }
    if let Some(first) = v.first_mut()
        && !first.style.code {
            first.text = first.text.trim_start().to_string();
        }
    if let Some(last) = v.last_mut()
        && !last.style.code {
            last.text = last.text.trim_end().to_string();
        }
    v
}

/// Turns bare http(s) links in ordinary text into clickable links.
fn linkify(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out = Vec::with_capacity(inlines.len());
    for inl in inlines {
        if inl.style.code || inl.style.link.is_some() || !inl.text.contains("://") {
            out.push(inl);
            continue;
        }
        let mut rest = inl.text.as_str();
        while let Some(start) = find_url_start(rest) {
            let after = &rest[start..];
            let mut end = after.find(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"').unwrap_or(after.len());
            while end > 0 && after[..end].ends_with(['.', ',', ';', ':', '!', '?', ')', '\'']) {
                end -= after[..end].chars().last().map(char::len_utf8).unwrap_or(1);
            }
            if end <= "https://".len() {
                break;
            }
            if start > 0 {
                out.push(Inline { text: rest[..start].to_string(), style: inl.style.clone() });
            }
            let url = &after[..end];
            out.push(Inline { text: url.to_string(), style: Style { link: Some(url.to_string()), ..inl.style.clone() } });
            rest = &after[end..];
        }
        if !rest.is_empty() {
            out.push(Inline { text: rest.to_string(), style: inl.style.clone() });
        }
    }
    out
}

fn find_url_start(s: &str) -> Option<usize> {
    let a = s.find("https://");
    let b = s.find("http://");
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    }
}

/// Replaces `:shortcode:` with the emoji, the way devzat's terminal renderer does.
pub fn replace_shortcodes(s: &str) -> String {
    if !s.contains(':') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(':') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-'))
            .unwrap_or(after.len());
        if name_len > 0 && after[name_len..].starts_with(':')
            && let Some(e) = emojis::get_by_shortcode(&after[..name_len]) {
                out.push_str(e.as_ref());
                rest = &after[name_len + 1..];
                continue;
            }
        out.push(':');
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_with_ansi_and_images() {
        let doc = parse("hey \x1b[38;5;218malice\x1b[39m **look** :rocket:\n![cat](https://x.test/cat.jpg)");
        assert_eq!(doc.blocks.len(), 2, "{doc:?}");
        let Block::Para(p) = &doc.blocks[0] else { panic!("{doc:?}") };
        let alice = p.iter().find(|i| i.text == "alice").expect("alice span");
        assert!(alice.style.ansi.fg.is_some());
        assert!(p.iter().any(|i| i.text == "look" && i.style.bold));
        assert!(p.iter().any(|i| i.text.contains('🚀')));
        assert_eq!(doc.images(), vec![("https://x.test/cat.jpg".to_string(), false)]);
    }

    #[test]
    fn hq_forms_and_links() {
        for text in ["!hq[](devzat-file:a.png)", "![hq](devzat-file:a.png)", "![hq: kitty](devzat-file:a.png)"] {
            let doc = parse(text);
            assert_eq!(doc.images(), vec![("devzat-file:a.png".to_string(), true)], "{text}");
        }
        let doc = parse("see https://example.com/page. ok");
        let Block::Para(p) = &doc.blocks[0] else { panic!() };
        assert!(p.iter().any(|i| i.style.link.as_deref() == Some("https://example.com/page")));
    }

    #[test]
    fn code_blocks_keep_text() {
        let doc = parse("```go\nfmt.Println(\"hi\")\n```");
        let Block::Code { lang, inlines } = &doc.blocks[0] else { panic!("{doc:?}") };
        assert_eq!(lang, "go");
        assert_eq!(inlines[0].text, "fmt.Println(\"hi\")");
    }
}
