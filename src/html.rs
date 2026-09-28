use crate::renderer::{Span, Style};
use crate::table::{Block, Cell, Row, TableModel};

const MAX_HTML_NEST: usize = 64;

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

#[derive(Clone, Debug, PartialEq)]
pub enum HtmlPiece {
    Table(TableModel),
    Raw(String),
}

#[derive(Clone, Debug)]
enum Node {
    Element { name: String, children: Vec<Node> },
    Text(String),
}

impl Drop for Node {
    fn drop(&mut self) {
        let mut stack: Vec<Node> = match self {
            Node::Element { children, .. } => std::mem::take(children),
            Node::Text(_) => Vec::new(),
        };
        while let Some(mut node) = stack.pop() {
            if let Node::Element { children, .. } = &mut node {
                stack.append(children);
            }
        }
    }
}

fn push_child(stack: &mut [(String, Vec<Node>)], root: &mut Vec<Node>, node: Node) {
    match stack.last_mut() {
        Some((_, children)) => children.push(node),
        None => root.push(node),
    }
}

fn autoclose_for(name: &str) -> &'static [&'static str] {
    match name {
        "tr" => &["tr"],
        "td" | "th" => &["td", "th"],
        "tbody" | "thead" | "tfoot" => &["tbody", "thead", "tfoot"],
        _ => &[],
    }
}

fn close_until(stack: &mut Vec<(String, Vec<Node>)>, root: &mut Vec<Node>, targets: &[&str]) {
    let floor = stack.iter().rposition(|(n, _)| n == "table").map_or(0, |i| i + 1);
    if let Some(pos) = stack[floor..].iter().rposition(|(n, _)| targets.contains(&n.as_str())) {
        let pos = floor + pos;
        while stack.len() > pos + 1 {
            let (n, c) = stack.pop().unwrap();
            push_child(stack, root, Node::Element { name: n, children: c });
        }
        let (n, c) = stack.pop().unwrap();
        push_child(stack, root, Node::Element { name: n, children: c });
    }
}

fn build_tree(tokens: &[Token]) -> Vec<Node> {
    let mut root: Vec<Node> = Vec::new();
    let mut stack: Vec<(String, Vec<Node>)> = Vec::new();
    for tok in tokens {
        match tok {
            Token::Text { text, .. } => push_child(&mut stack, &mut root, Node::Text(text.clone())),
            Token::Start { name, self_closing, .. } => {
                if *self_closing {
                    push_child(&mut stack, &mut root, Node::Element { name: name.clone(), children: Vec::new() });
                } else {
                    close_until(&mut stack, &mut root, autoclose_for(name));
                    stack.push((name.clone(), Vec::new()));
                }
            }
            Token::End { name, .. } => {
                if let Some(pos) = stack.iter().rposition(|(n, _)| n == name) {
                    while stack.len() > pos + 1 {
                        let (n, c) = stack.pop().unwrap();
                        push_child(&mut stack, &mut root, Node::Element { name: n, children: c });
                    }
                    let (n, c) = stack.pop().unwrap();
                    push_child(&mut stack, &mut root, Node::Element { name: n, children: c });
                }
            }
        }
    }
    while let Some((n, c)) = stack.pop() {
        push_child(&mut stack, &mut root, Node::Element { name: n, children: c });
    }
    root
}

fn flush_text(cur: &mut Vec<Span>, out: &mut Vec<Block>) {
    if !cur.is_empty() {
        out.push(Block::Text(std::mem::take(cur)));
    }
}

fn push_span(cur: &mut Vec<Span>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = cur.last_mut() {
        if last.style == style {
            last.text.push_str(text);
            return;
        }
    }
    cur.push(Span { text: text.to_string(), style });
}

fn walk_inline(nodes: &[Node], style: Style, depth: usize, max_depth: usize, nest: usize, cur: &mut Vec<Span>, out: &mut Vec<Block>) {
    for node in nodes {
        match node {
            Node::Text(t) => push_span(cur, t, style),
            Node::Element { name, children } => {
                if nest >= MAX_HTML_NEST {
                    collect_text(children, style, cur);
                    continue;
                }
                match name.as_str() {
                    "br" => flush_text(cur, out),
                    "table" => {
                        flush_text(cur, out);
                        if depth >= max_depth {
                            walk_inline(children, style, depth, max_depth, nest + 1, cur, out);
                        } else {
                            out.push(Block::Table(convert_table(children, depth + 1, max_depth)));
                        }
                    }
                    "b" | "strong" => walk_inline(children, Style { bold: true, ..style }, depth, max_depth, nest + 1, cur, out),
                    "i" | "em" => walk_inline(children, Style { italic: true, ..style }, depth, max_depth, nest + 1, cur, out),
                    "code" => walk_inline(children, Style { code: true, ..style }, depth, max_depth, nest + 1, cur, out),
                    "s" | "del" | "strike" => walk_inline(children, Style { strike: true, ..style }, depth, max_depth, nest + 1, cur, out),
                    "script" | "style" => {}
                    _ => walk_inline(children, style, depth, max_depth, nest + 1, cur, out),
                }
            }
        }
    }
}

fn collect_text(children: &[Node], style: Style, cur: &mut Vec<Span>) {
    let mut stack: Vec<&Node> = children.iter().rev().collect();
    while let Some(node) = stack.pop() {
        match node {
            Node::Text(t) => push_span(cur, t, style),
            Node::Element { children, .. } => {
                for c in children.iter().rev() {
                    stack.push(c);
                }
            }
        }
    }
}

fn cell_blocks(nodes: &[Node], depth: usize, max_depth: usize) -> Vec<Block> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    walk_inline(nodes, Style::default(), depth, max_depth, 0, &mut cur, &mut out);
    flush_text(&mut cur, &mut out);
    out
}

fn convert_row(children: &[Node], in_thead: bool, cell_depth: usize, max_depth: usize) -> Row {
    let mut cells = Vec::new();
    let mut any_th = false;
    for node in children {
        if let Node::Element { name, children } = node {
            if name == "td" || name == "th" {
                if name == "th" {
                    any_th = true;
                }
                cells.push(Cell { blocks: cell_blocks(children, cell_depth, max_depth) });
            }
        }
    }
    Row { cells, header: in_thead || any_th }
}

fn collect_rows(nodes: &[Node], in_thead: bool, cell_depth: usize, max_depth: usize, rows: &mut Vec<Row>) {
    for node in nodes {
        if let Node::Element { name, children } = node {
            match name.as_str() {
                "tr" => rows.push(convert_row(children, in_thead, cell_depth, max_depth)),
                "thead" => collect_rows(children, true, cell_depth, max_depth, rows),
                "tbody" | "tfoot" => collect_rows(children, false, cell_depth, max_depth, rows),
                _ => {}
            }
        }
    }
}

fn convert_table(nodes: &[Node], depth: usize, max_depth: usize) -> TableModel {
    let mut rows = Vec::new();
    collect_rows(nodes, false, depth, max_depth, &mut rows);
    TableModel { rows, aligns: Vec::new() }
}

fn text_content(segment: &str) -> String {
    let mut out = String::new();
    let mut skip = 0usize;
    for tok in tokenize(segment) {
        match tok {
            Token::Start { name, .. } if name == "script" || name == "style" => skip += 1,
            Token::End { name, .. } if name == "script" || name == "style" => skip = skip.saturating_sub(1),
            Token::Text { text, .. } if skip == 0 => out.push_str(&text),
            _ => {}
        }
    }
    out
}

fn strip_script_style(tokens: Vec<Token>) -> Vec<Token> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut skip = 0usize;
    for tok in tokens {
        match &tok {
            Token::Start { name, self_closing, .. } if name == "script" || name == "style" => {
                if !*self_closing {
                    skip += 1;
                }
            }
            Token::End { name, .. } if name == "script" || name == "style" => {
                skip = skip.saturating_sub(1);
            }
            _ if skip == 0 => out.push(tok),
            _ => {}
        }
    }
    out
}

pub fn has_unterminated_table(src: &str) -> bool {
    let tokens = strip_script_style(tokenize(src));
    let mut depth = 0usize;
    for tok in &tokens {
        match tok {
            Token::Start { name, self_closing, .. } if name == "table" && !*self_closing => depth += 1,
            Token::End { name, .. } if name == "table" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth > 0
}

fn top_level_tables(tokens: &[Token], src_len: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for tok in tokens {
        match tok {
            Token::Start { name, self_closing, start: s, .. } if name == "table" && !*self_closing => {
                if depth == 0 {
                    start = *s;
                }
                depth += 1;
            }
            Token::End { name, end: e, .. } if name == "table" => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    spans.push((start, *e));
                }
            }
            _ => {}
        }
    }
    if depth > 0 {
        spans.push((start, src_len));
    }
    spans
}

pub fn parse_block(src: &str, max_depth: usize) -> Vec<HtmlPiece> {
    let tokens = strip_script_style(tokenize(src));
    let spans = top_level_tables(&tokens, src.len());
    if spans.is_empty() {
        return vec![HtmlPiece::Raw(src.to_string())];
    }
    let mut pieces = Vec::new();
    let mut prev = 0usize;
    for (s, e) in spans {
        if s > prev {
            let raw = text_content(&src[prev..s]);
            if !raw.trim().is_empty() {
                pieces.push(HtmlPiece::Raw(raw));
            }
        }
        let sub: Vec<Token> = tokens
            .iter()
            .filter(|t| {
                let (ts, te) = token_bounds(t);
                ts >= s && te <= e
            })
            .cloned()
            .collect();
        let tree = build_tree(&sub);
        if let Some(Node::Element { name, children }) = tree.first() {
            if name == "table" {
                let model = convert_table(children, 1, max_depth);
                if model.ncols() == 0 {
                    pieces.push(HtmlPiece::Raw(src[s..e].to_string()));
                } else {
                    pieces.push(HtmlPiece::Table(model));
                }
            }
        }
        prev = e;
    }
    if prev < src.len() {
        let raw = text_content(&src[prev..]);
        if !raw.trim().is_empty() {
            pieces.push(HtmlPiece::Raw(raw));
        }
    }
    pieces
}

fn token_bounds(t: &Token) -> (usize, usize) {
    match t {
        Token::Start { start, end, .. } | Token::End { start, end, .. } | Token::Text { start, end, .. } => (*start, *end),
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

    fn tbl(rows: &[(&[&str], bool)]) -> TableModel {
        TableModel {
            rows: rows
                .iter()
                .map(|(cells, header)| Row {
                    cells: cells
                        .iter()
                        .map(|c| Cell {
                            blocks: vec![Block::Text(vec![Span { text: c.to_string(), style: Style::default() }])],
                        })
                        .collect(),
                    header: *header,
                })
                .collect(),
            aligns: vec![],
        }
    }

    #[test]
    fn no_table_is_passthrough_raw() {
        assert_eq!(parse_block("<div>hi</div>", 8), vec![HtmlPiece::Raw("<div>hi</div>".to_string())]);
    }

    #[test]
    fn parses_simple_table() {
        let pieces = parse_block("<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>", 8);
        assert_eq!(pieces, vec![HtmlPiece::Table(tbl(&[(&["a", "b"], true), (&["1", "2"], false)]))]);
    }

    #[test]
    fn wrapper_tags_are_transparent_around_table() {
        let pieces = parse_block("<div>A<table><tr><td>x</td></tr></table>B</div>", 8);
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[0], HtmlPiece::Raw("A".to_string()));
        assert!(matches!(pieces[1], HtmlPiece::Table(_)));
        assert_eq!(pieces[2], HtmlPiece::Raw("B".to_string()));
    }

    #[test]
    fn nested_table_is_recursive() {
        let src = "<table><tr><td><table><tr><td>x</td></tr></table></td></tr></table>";
        let pieces = parse_block(src, 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => match &t.rows[0].cells[0].blocks[0] {
                Block::Table(inner) => assert_eq!(inner.rows[0].cells[0].blocks.len(), 1),
                other => panic!("expected inner table, got {:?}", other),
            },
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn br_splits_text_blocks() {
        let pieces = parse_block("<table><tr><td>a<br>b</td></tr></table>", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => assert_eq!(t.rows[0].cells[0].blocks.len(), 2),
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn inline_tags_become_styles() {
        let pieces = parse_block("<table><tr><td><b>x</b><i>y</i></td></tr></table>", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => {
                let spans = match &t.rows[0].cells[0].blocks[0] {
                    Block::Text(s) => s,
                    other => panic!("expected text, got {:?}", other),
                };
                assert!(spans.iter().any(|s| s.style.bold && s.text == "x"));
                assert!(spans.iter().any(|s| s.style.italic && s.text == "y"));
            }
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn thead_marks_header_rows() {
        let pieces = parse_block("<table><thead><tr><td>h</td></tr></thead><tbody><tr><td>b</td></tr></tbody></table>", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => {
                assert!(t.rows[0].header);
                assert!(!t.rows[1].header);
            }
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn multiple_tables_keep_order() {
        let src = "<table><tr><td>a</td></tr></table>mid<table><tr><td>b</td></tr></table>";
        let pieces = parse_block(src, 8);
        assert!(matches!(pieces[0], HtmlPiece::Table(_)));
        assert_eq!(pieces[1], HtmlPiece::Raw("mid".to_string()));
        assert!(matches!(pieces[2], HtmlPiece::Table(_)));
    }

    #[test]
    fn stray_end_table_does_not_duplicate() {
        let pieces = parse_block("<table><tr><td>a</td></tr></table></table>", 8);
        assert_eq!(pieces.iter().filter(|p| matches!(p, HtmlPiece::Table(_))).count(), 1);
    }

    #[test]
    fn self_closing_table_does_not_poison() {
        let pieces = parse_block("<table/><table><tr><td>a</td></tr></table>", 8);
        assert_eq!(pieces.iter().filter(|p| matches!(p, HtmlPiece::Table(_))).count(), 1);
    }

    #[test]
    fn deeply_nested_html_does_not_overflow() {
        let mut s = String::from("<table><tr><td>");
        for _ in 0..50000 {
            s.push_str("<div>");
        }
        s.push_str("deep");
        for _ in 0..50000 {
            s.push_str("</div>");
        }
        s.push_str("</td></tr></table>");
        let pieces = parse_block(&s, 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => {
                let text: String = match &t.rows[0].cells[0].blocks[0] {
                    Block::Text(spans) => spans.iter().map(|sp| sp.text.as_str()).collect(),
                    other => panic!("expected text, got {:?}", other),
                };
                assert!(text.contains("deep"));
            }
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn empty_table_falls_back_to_raw() {
        assert_eq!(
            parse_block("<table></table>", 8),
            vec![HtmlPiece::Raw("<table></table>".to_string())]
        );
    }

    #[test]
    fn script_and_style_content_is_dropped() {
        let pieces = parse_block("<table><tr><td><script>alert(1)</script>hi</td></tr></table>", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => {
                let text: String = match &t.rows[0].cells[0].blocks[0] {
                    Block::Text(spans) => spans.iter().map(|s| s.text.as_str()).collect(),
                    other => panic!("expected text, got {:?}", other),
                };
                assert_eq!(text, "hi");
            }
            other => panic!("expected table, got {:?}", other),
        }
        assert_eq!(
            parse_block("<table></table><style>x{}</style>", 8),
            vec![HtmlPiece::Raw("<table></table>".to_string())]
        );
    }

    #[test]
    fn optional_end_tags_are_implicitly_closed() {
        let pieces = parse_block("<table><tr><td>a</td><tr><td>b</td></table>", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => {
                assert_eq!(t.rows.len(), 2, "both rows must survive: {:?}", t);
                assert_eq!(t.rows[1].cells.len(), 1);
            }
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn missing_td_close_still_splits_cells() {
        let pieces = parse_block("<table><tr><td>a<td>b</td></tr></table>", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => assert_eq!(t.rows[0].cells.len(), 2, "cells must split: {:?}", t),
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn unterminated_table_still_converts() {
        let pieces = parse_block("<table><tr><td>hello", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => {
                assert_eq!(t.rows.len(), 1);
                let text: String = match &t.rows[0].cells[0].blocks[0] {
                    Block::Text(spans) => spans.iter().map(|s| s.text.as_str()).collect(),
                    other => panic!("expected text, got {:?}", other),
                };
                assert_eq!(text, "hello");
            }
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn script_containing_table_is_not_rendered() {
        let pieces = parse_block("<script>var s=\"<table><tr><td>x</td></tr></table>\";</script>", 8);
        assert!(
            pieces.iter().all(|p| matches!(p, HtmlPiece::Raw(_))),
            "table inside script must not render: {:?}",
            pieces
        );
    }

    #[test]
    fn omitted_close_then_new_row_keeps_rows() {
        let pieces = parse_block("<table><tr><td>a<td>b<tr><td>c<td>d</table>", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => {
                assert_eq!(t.rows.len(), 2, "both rows must survive: {:?}", t);
                assert_eq!(t.rows[0].cells.len(), 2);
                assert_eq!(t.rows[1].cells.len(), 2);
            }
            other => panic!("expected table, got {:?}", other),
        }
    }

    #[test]
    fn omitted_cell_close_before_tbody_keeps_header() {
        let pieces = parse_block("<table><thead><tr><th>h1<th>h2<tbody><tr><td>a<td>b</table>", 8);
        match &pieces[0] {
            HtmlPiece::Table(t) => {
                assert!(t.rows[0].header, "first row must be header: {:?}", t);
                assert!(!t.rows[1].header, "body row must not be header: {:?}", t);
                assert_eq!(t.rows[0].cells.len(), 2);
                assert_eq!(t.rows[1].cells.len(), 2);
            }
            other => panic!("expected table, got {:?}", other),
        }
    }
}
