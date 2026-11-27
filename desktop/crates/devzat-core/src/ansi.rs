//! Turns ANSI-colored text (how devzat colors names, mentions, and bot output) into styled spans.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct AnsiStyle {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub inverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnsiSpan {
    pub text: String,
    pub style: AnsiStyle,
}

const BASE16: [Rgb; 16] = [
    Rgb(0x1d, 0x1f, 0x21),
    Rgb(0xcc, 0x44, 0x44),
    Rgb(0x5f, 0xb8, 0x5f),
    Rgb(0xd7, 0xb0, 0x3c),
    Rgb(0x4a, 0x8c, 0xe6),
    Rgb(0xb8, 0x66, 0xd6),
    Rgb(0x3f, 0xb8, 0xc0),
    Rgb(0xc8, 0xc8, 0xc8),
    Rgb(0x6b, 0x6f, 0x76),
    Rgb(0xff, 0x6b, 0x6b),
    Rgb(0x8a, 0xe2, 0x34),
    Rgb(0xff, 0xd8, 0x4d),
    Rgb(0x6f, 0xb3, 0xff),
    Rgb(0xe3, 0x8c, 0xff),
    Rgb(0x5e, 0xe3, 0xea),
    Rgb(0xf5, 0xf5, 0xf5),
];

/// The xterm 256-color palette.
pub fn xterm256(n: u8) -> Rgb {
    match n {
        0..=15 => BASE16[n as usize],
        16..=231 => {
            let i = n - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
        }
        232..=255 => {
            let v = 8 + (n - 232) * 10;
            Rgb(v, v, v)
        }
    }
}

/// Applies one SGR (`ESC [ ... m`) parameter list to a style.
pub fn apply_sgr(style: &mut AnsiStyle, params: &[u16]) {
    if params.is_empty() {
        *style = AnsiStyle::default();
        return;
    }
    let mut i = 0;
    while i < params.len() {
        let p = params[i];
        match p {
            0 => *style = AnsiStyle::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            7 => style.inverse = true,
            9 => style.strike = true,
            21 | 22 => {
                style.bold = false;
                style.dim = false;
            }
            23 => style.italic = false,
            24 => style.underline = false,
            27 => style.inverse = false,
            29 => style.strike = false,
            30..=37 => style.fg = Some(BASE16[(p - 30) as usize]),
            39 => style.fg = None,
            40..=47 => style.bg = Some(BASE16[(p - 40) as usize]),
            49 => style.bg = None,
            90..=97 => style.fg = Some(BASE16[(p - 90 + 8) as usize]),
            100..=107 => style.bg = Some(BASE16[(p - 100 + 8) as usize]),
            38 | 48 => {
                let color = match params.get(i + 1) {
                    Some(5) => {
                        let c = params.get(i + 2).map(|&n| xterm256(n.min(255) as u8));
                        i += 2;
                        c
                    }
                    Some(2) => {
                        let get = |k: usize| params.get(i + k).copied().unwrap_or(0).min(255) as u8;
                        let c = Some(Rgb(get(2), get(3), get(4)));
                        i += 4;
                        c
                    }
                    _ => None,
                };
                if p == 38 {
                    style.fg = color;
                } else {
                    style.bg = color;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

pub enum Token<'a> {
    Text(&'a str),
    /// The parameters of an SGR (color/style) sequence.
    Sgr(Vec<u16>),
}

/// Splits text into printable runs and SGR sequences. Other escape sequences and control characters
/// (bells, cursor movement, window titles) are dropped; newlines and tabs are kept.
pub struct Tokens<'a> {
    s: &'a str,
    i: usize,
}

pub fn tokens(s: &str) -> Tokens<'_> {
    Tokens { s, i: 0 }
}

impl<'a> Iterator for Tokens<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        let bytes = self.s.as_bytes();
        loop {
            if self.i >= bytes.len() {
                return None;
            }
            let start = self.i;
            let b = bytes[start];
            if b == 0x1b {
                let (consumed, sgr) = parse_escape(&bytes[start..]);
                self.i += consumed;
                if let Some(params) = sgr {
                    return Some(Token::Sgr(params));
                }
                continue;
            }
            if is_dropped_control(b) {
                self.i += 1;
                continue;
            }
            let mut end = start;
            while end < bytes.len() && bytes[end] != 0x1b && !is_dropped_control(bytes[end]) {
                end += 1;
            }
            self.i = end;
            return Some(Token::Text(&self.s[start..end]));
        }
    }
}

fn is_dropped_control(b: u8) -> bool {
    (b < 0x20 && b != b'\n' && b != b'\t') || b == 0x7f
}

fn parse_escape(b: &[u8]) -> (usize, Option<Vec<u16>>) {
    match b.get(1) {
        Some(b'[') => {
            let mut j = 2;
            while j < b.len() && !(0x40..=0x7e).contains(&b[j]) {
                j += 1;
            }
            if j >= b.len() {
                return (b.len(), None);
            }
            if b[j] != b'm' {
                return (j + 1, None);
            }
            let params = std::str::from_utf8(&b[2..j])
                .unwrap_or("")
                .split([';', ':'])
                .filter(|p| !p.is_empty())
                .map(|p| p.parse::<u16>().unwrap_or(0))
                .collect();
            (j + 1, Some(params))
        }
        Some(b']') => {
            let mut j = 2;
            while j < b.len() {
                if b[j] == 0x07 {
                    return (j + 1, None);
                }
                if b[j] == 0x1b && b.get(j + 1) == Some(&b'\\') {
                    return (j + 2, None);
                }
                j += 1;
            }
            (b.len(), None)
        }
        Some(&c) if c >= 0x80 => (1, None),
        Some(_) => (2, None),
        None => (1, None),
    }
}

pub fn parse(s: &str) -> Vec<AnsiSpan> {
    let mut spans: Vec<AnsiSpan> = Vec::new();
    let mut style = AnsiStyle::default();
    for token in tokens(s) {
        match token {
            Token::Sgr(params) => apply_sgr(&mut style, &params),
            Token::Text(t) => match spans.last_mut() {
                Some(last) if last.style == style => last.text.push_str(t),
                _ => spans.push(AnsiSpan { text: t.to_string(), style }),
            },
        }
    }
    spans
}

pub fn strip(s: &str) -> String {
    tokens(s)
        .filter_map(|t| match t {
            Token::Text(t) => Some(t),
            Token::Sgr(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_strips() {
        let spans = parse("\x1b[38;5;218malice\x1b[39m has \x1b[1mjoined\x1b[0m\x07");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "alice");
        assert_eq!(spans[0].style.fg, Some(xterm256(218)));
        assert_eq!(spans[1].text, " has ");
        assert!(spans[2].style.bold);
        assert_eq!(strip("\x1b]0;title\x07hi\x1b[2Jthere"), "hithere");
        let tc = parse("\x1b[38;2;10;20;30mx");
        assert_eq!(tc[0].style.fg, Some(Rgb(10, 20, 30)));
    }
}
