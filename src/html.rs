#![allow(dead_code)]

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
    "track", "wbr",
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Start { name: String, self_closing: bool, start: usize, end: usize },
    End { name: String, start: usize, end: usize },
    Text { text: String, start: usize, end: usize },
}

fn is_void(name: &str) -> bool {
    VOID.contains(&name)
}

fn tag_name(inner: &str) -> String {
    inner
        .trim()
        .trim_start_matches('/')
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn find_tag_end(src: &str, start: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => {
                if b == b'"' || b == b'\'' {
                    quote = Some(b);
                } else if b == b'>' {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

fn tokenize(src: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut text_start = 0usize;
    let bytes = src.as_bytes();
    let mut i = 0usize;

    let flush = |tokens: &mut Vec<Token>, text: &mut String, start: usize, end: usize| {
        if !text.is_empty() {
            tokens.push(Token::Text { text: std::mem::take(text), start, end });
        }
    };

    while i < bytes.len() {
        if bytes[i] != b'<' {
            let next = src[i..].find('<').map(|o| i + o).unwrap_or(bytes.len());
            if text.is_empty() {
                text_start = i;
            }
            text.push_str(&decode_entities(&src[i..next]));
            i = next;
            continue;
        }
        if src[i..].starts_with("<!--") {
            flush(&mut tokens, &mut text, text_start, i);
            i = match src[i + 4..].find("-->") {
                Some(o) => i + 4 + o + 3,
                None => bytes.len(),
            };
            continue;
        }
        if src[i..].starts_with("<!") {
            flush(&mut tokens, &mut text, text_start, i);
            i = match src[i..].find('>') {
                Some(o) => i + o + 1,
                None => bytes.len(),
            };
            continue;
        }
        let rest = &src[i + 1..];
        let name_body = rest.strip_prefix('/').unwrap_or(rest);
        let valid_name = name_body.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
        if !valid_name {
            if text.is_empty() {
                text_start = i;
            }
            text.push('<');
            i += 1;
            continue;
        }
        match find_tag_end(src, i) {
            Some(tag_end_idx) => {
                let tag_end = tag_end_idx + 1;
                let inner = &src[i + 1..tag_end_idx];
                flush(&mut tokens, &mut text, text_start, i);
                if let Some(name) = closing_name(inner) {
                    tokens.push(Token::End { name, start: i, end: tag_end });
                } else {
                    let name = tag_name(inner);
                    if !name.is_empty() {
                        let self_closing = inner.trim_end().ends_with('/') || is_void(&name);
                        tokens.push(Token::Start { name, self_closing, start: i, end: tag_end });
                    }
                }
                i = tag_end;
            }
            None => {
                if text.is_empty() {
                    text_start = i;
                }
                text.push_str(&decode_entities(&src[i..]));
                i = bytes.len();
            }
        }
    }
    flush(&mut tokens, &mut text, text_start, bytes.len());
    tokens
}

fn closing_name(inner: &str) -> Option<String> {
    let t = inner.trim();
    t.strip_prefix('/').map(tag_name)
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut i = 0usize;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'&' {
            let mut window_end = (i + 34).min(s.len());
            while window_end < s.len() && !s.is_char_boundary(window_end) {
                window_end -= 1;
            }
            if let Some(rel) = s[i..window_end].find(';') {
                let semi = i + rel;
                if let Some(ch) = decode_entity(&s[i + 1..semi]) {
                    out.push_str(&ch);
                    i = semi + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn decode_entity(ent: &str) -> Option<String> {
    match ent {
        "amp" => Some("&".to_string()),
        "lt" => Some("<".to_string()),
        "gt" => Some(">".to_string()),
        "quot" => Some("\"".to_string()),
        "apos" => Some("'".to_string()),
        "nbsp" => Some(" ".to_string()),
        _ => {
            let num = ent.strip_prefix('#')?;
            let hex = num.strip_prefix('x').or_else(|| num.strip_prefix('X'));
            let code = if let Some(hex) = hex {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num.parse::<u32>().ok()
            };
            code.and_then(char::from_u32).map(|c| c.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_tags_and_text() {
        let toks = tokenize("<td>a</td>");
        assert_eq!(toks.len(), 3);
        assert!(matches!(&toks[0], Token::Start { name, .. } if name == "td"));
        assert!(matches!(&toks[1], Token::Text { text, .. } if text == "a"));
        assert!(matches!(&toks[2], Token::End { name, .. } if name == "td"));
    }

    #[test]
    fn void_elements_are_self_closing() {
        let toks = tokenize("<br>x");
        assert!(matches!(&toks[0], Token::Start { name, self_closing, .. } if name == "br" && *self_closing));
    }

    #[test]
    fn drops_comments_and_doctype() {
        assert!(tokenize("<!DOCTYPE html><!-- c -->hi").iter().all(|t| matches!(t, Token::Text { .. })));
    }

    #[test]
    fn decodes_named_and_numeric_entities() {
        assert_eq!(decode_entities("a &amp; b &#65; &#x42;"), "a & b A B");
        assert_eq!(decode_entities("&unknown;"), "&unknown;");
    }

    #[test]
    fn incomplete_tag_does_not_panic() {
        let toks = tokenize("<table><tr><td>a");
        assert!(!toks.is_empty());
    }

    #[test]
    fn stray_less_than_is_text() {
        let toks = tokenize("<td>a < b</td>");
        assert!(toks.iter().any(|t| matches!(t, Token::End { name, .. } if name == "td")));
    }

    #[test]
    fn quoted_gt_in_attribute_does_not_truncate() {
        let toks = tokenize("<td title=\"a>b\">z</td>");
        assert!(matches!(&toks[0], Token::Start { name, .. } if name == "td"));
        assert!(matches!(&toks[1], Token::Text { text, .. } if text == "z"));
        assert!(matches!(&toks[2], Token::End { name, .. } if name == "td"));
    }

    #[test]
    fn offsets_are_char_boundaries_and_pin_text() {
        let src = "<td>中&amp;</td>";
        let toks = tokenize(src);
        for t in &toks {
            let (s, e) = match t {
                Token::Start { start, end, .. }
                | Token::End { start, end, .. }
                | Token::Text { start, end, .. } => (*start, *end),
            };
            assert!(src.is_char_boundary(s), "start {s} not boundary");
            assert!(src.is_char_boundary(e), "end {e} not boundary");
            assert!(s <= e && e <= src.len());
        }
        assert!(matches!(&toks[1], Token::Text { text, .. } if text == "中&"));
    }

    #[test]
    fn numeric_entity_overflow_is_literal() {
        assert_eq!(decode_entities("&#99999999999; &#xD800;"), "&#99999999999; &#xD800;");
    }

    #[test]
    fn incomplete_lt_decodes_entities() {
        assert_eq!(
            tokenize("< &amp;")[0],
            Token::Text { text: "< &".to_string(), start: 0, end: 7 }
        );
    }
}
