//! Comment/string masking and line statistics.
//!
//! Produces two views of a source file with identical line structure:
//! - `code`: comments replaced by spaces, string literals intact (used to read import specifiers)
//! - `blank`: comments *and* string contents replaced by spaces (used for symbols/complexity so
//!   keywords inside strings or comments are not counted)
//!
//! This is a lexical approximation; exotic constructs (heredocs, nested template literals,
//! shell `$#`) may be mis-classified. Results derived from it are labelled *estimated*.

use super::Syntax;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineStats {
    /// Exact: number of lines.
    pub total: u32,
    /// Exact: whitespace-only lines.
    pub blank: u32,
    /// Estimated: lines containing only comments.
    pub comment: u32,
    /// Estimated: remaining lines.
    pub code: u32,
}

pub struct Masked {
    pub code: String,
    pub blank: String,
    pub stats: LineStats,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Normal,
    Line,
    Block(usize, u32), // block index, nesting depth
    Str(char),
    Triple(char),
    RawStr(usize), // number of '#'
}

fn starts(chars: &[char], i: usize, pat: &str) -> bool {
    let n = pat.chars().count();
    i + n <= chars.len() && chars[i..i + n].iter().copied().eq(pat.chars())
}

/// Push spaces occupying the same number of bytes as c (keeps byte offsets aligned).
fn sp(s: &mut String, c: char) {
    for _ in 0..c.len_utf8() {
        s.push(' ');
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn mask(src: &str, syn: &Syntax) -> Masked {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut code = String::with_capacity(src.len());
    let mut blank = String::with_capacity(src.len());
    let mut state = State::Normal;

    // Per-line flags.
    let mut line_has_code = false;
    let mut line_has_comment = false;
    let mut stats = LineStats::default();
    let finish_line = |has_code: bool, has_comment: bool, stats: &mut LineStats| {
        stats.total += 1;
        if has_code {
            stats.code += 1;
        } else if has_comment {
            stats.comment += 1;
        } else {
            stats.blank += 1;
        }
    };

    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c == '\n' {
            code.push('\n');
            blank.push('\n');
            finish_line(line_has_code, line_has_comment, &mut stats);
            line_has_code = false;
            line_has_comment = false;
            if state == State::Line {
                state = State::Normal;
            }
            if let State::Str(q) = state {
                if q != '`' {
                    state = State::Normal; // unterminated single-line string
                }
            }
            i += 1;
            continue;
        }
        if c == '\r' {
            code.push('\r');
            blank.push('\r');
            i += 1;
            continue;
        }
        match state {
            State::Normal => {
                // Block comments (checked before line comments: `--[[` vs `--`).
                if let Some((bi, (open, _))) = syn.block.iter().enumerate().find(|(_, (o, _))| starts(&chars, i, o)) {
                    let len = open.chars().count();
                    for _ in 0..len {
                        code.push(' ');
                        blank.push(' ');
                    }
                    line_has_comment = true;
                    state = State::Block(bi, 1);
                    i += len;
                    continue;
                }
                if let Some(lc) = syn.line.iter().find(|l| starts(&chars, i, l)) {
                    let _ = lc;
                    line_has_comment = true;
                    state = State::Line;
                    code.push(' ');
                    blank.push(' ');
                    i += 1;
                    continue;
                }
                if syn.triple_quotes && (c == '"' || c == '\'') && starts(&chars, i, &c.to_string().repeat(3)) {
                    for _ in 0..3 {
                        code.push(c);
                        blank.push(c);
                    }
                    line_has_code = true;
                    state = State::Triple(c);
                    i += 3;
                    continue;
                }
                if syn.rust_chars {
                    // Raw strings: r"..", r#".."#, br#".."#
                    let prev_ident = i > 0 && is_ident(chars[i - 1]);
                    if !prev_ident && (c == 'r' || (c == 'b' && i + 1 < n && chars[i + 1] == 'r')) {
                        let mut j = if c == 'b' { i + 2 } else { i + 1 };
                        let mut hashes = 0;
                        while j < n && chars[j] == '#' {
                            hashes += 1;
                            j += 1;
                        }
                        if j < n && chars[j] == '"' && (hashes > 0 || j == i + 1 || (c == 'b' && j == i + 2)) {
                            for &ch in &chars[i..=j] {
                                code.push(ch);
                                blank.push(ch);
                            }
                            line_has_code = true;
                            state = State::RawStr(hashes);
                            i = j + 1;
                            continue;
                        }
                    }
                    if c == '\'' {
                        // char literal ('a', '\n', '\u{1F600}') vs lifetime ('a)
                        let is_char = (i + 1 < n && chars[i + 1] == '\\') || (i + 2 < n && chars[i + 2] == '\'');
                        if is_char {
                            let mut j = i + 1;
                            if chars[j] == '\\' {
                                j += 2;
                            } else {
                                j += 1;
                            }
                            while j < n && chars[j] != '\'' && chars[j] != '\n' {
                                j += 1;
                            }
                            let end = j.min(n - 1);
                            for &ch in &chars[i..=end] {
                                code.push(ch);
                                if ch == '\'' {
                                    blank.push('\'')
                                } else {
                                    sp(&mut blank, ch)
                                }
                            }
                            line_has_code = true;
                            i = end + 1;
                            continue;
                        }
                        code.push(c);
                        blank.push(c);
                        line_has_code = true;
                        i += 1;
                        continue;
                    }
                }
                if syn.quotes.contains(&c) {
                    code.push(c);
                    blank.push(c);
                    line_has_code = true;
                    state = State::Str(c);
                    i += 1;
                    continue;
                }
                code.push(c);
                blank.push(c);
                if !c.is_whitespace() {
                    line_has_code = true;
                }
                i += 1;
            }
            State::Line => {
                sp(&mut code, c);
                sp(&mut blank, c);
                i += 1;
            }
            State::Block(bi, depth) => {
                let (open, close) = syn.block[bi];
                if starts(&chars, i, close) {
                    let len = close.chars().count();
                    for _ in 0..len {
                        code.push(' ');
                        blank.push(' ');
                    }
                    i += len;
                    state = if depth > 1 { State::Block(bi, depth - 1) } else { State::Normal };
                    continue;
                }
                if syn.nested_block && starts(&chars, i, open) {
                    let len = open.chars().count();
                    for _ in 0..len {
                        code.push(' ');
                        blank.push(' ');
                    }
                    i += len;
                    state = State::Block(bi, depth + 1);
                    continue;
                }
                if !c.is_whitespace() {
                    line_has_comment = true;
                }
                sp(&mut code, c);
                sp(&mut blank, c);
                i += 1;
            }
            State::Str(q) => {
                line_has_code = true;
                if c == '\\' && i + 1 < n && chars[i + 1] != '\n' {
                    code.push(c);
                    code.push(chars[i + 1]);
                    blank.push(' ');
                    sp(&mut blank, chars[i + 1]);
                    i += 2;
                    continue;
                }
                code.push(c);
                if c == q {
                    blank.push(c);
                    state = State::Normal;
                } else {
                    sp(&mut blank, c);
                }
                i += 1;
            }
            State::Triple(q) => {
                line_has_code = true;
                if c == '\\' && i + 1 < n && chars[i + 1] != '\n' {
                    code.push(c);
                    code.push(chars[i + 1]);
                    blank.push(' ');
                    sp(&mut blank, chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == q && starts(&chars, i, &q.to_string().repeat(3)) {
                    for _ in 0..3 {
                        code.push(q);
                        blank.push(q);
                    }
                    state = State::Normal;
                    i += 3;
                    continue;
                }
                code.push(c);
                sp(&mut blank, c);
                i += 1;
            }
            State::RawStr(hashes) => {
                line_has_code = true;
                if c == '"' && (1..=hashes).all(|k| i + k < n && chars[i + k] == '#') {
                    for &ch in &chars[i..=i + hashes] {
                        code.push(ch);
                        blank.push(ch);
                    }
                    state = State::Normal;
                    i += hashes + 1;
                    continue;
                }
                code.push(c);
                sp(&mut blank, c);
                i += 1;
            }
        }
    }
    if n > 0 && chars[n - 1] != '\n' {
        finish_line(line_has_code, line_has_comment, &mut stats);
    }
    Masked { code, blank, stats }
}

/// Byte offsets of line starts, for converting match offsets to 1-based line numbers.
pub struct LineIndex(Vec<usize>);

impl LineIndex {
    pub fn new(s: &str) -> Self {
        let mut v = vec![0];
        for (i, b) in s.bytes().enumerate() {
            if b == b'\n' {
                v.push(i + 1);
            }
        }
        LineIndex(v)
    }
    pub fn line_of(&self, offset: usize) -> u32 {
        match self.0.binary_search(&offset) {
            Ok(i) => i as u32 + 1,
            Err(i) => i as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::by_id;

    fn m(lang: &str, src: &str) -> Masked {
        mask(src, &by_id(lang).unwrap().syntax)
    }

    #[test]
    fn js_comments_removed_strings_kept() {
        let r = m(
            "javascript",
            "import a from './a'; // import b from './b'\n/* import c from './c' */\nconst s = \"// not a comment\";\n",
        );
        assert!(r.code.contains("'./a'"));
        assert!(!r.code.contains("./b"));
        assert!(!r.code.contains("./c"));
        assert!(r.code.contains("// not a comment"));
        assert!(!r.blank.contains("not a comment"));
        assert_eq!(r.stats, LineStats { total: 3, blank: 0, comment: 1, code: 2 });
        assert_eq!(r.code.lines().count(), 3);
    }

    #[test]
    fn rust_lifetimes_chars_raw_strings_and_nested_comments() {
        let src = "fn f<'a>(x: &'a str) -> char { 'x' }\n/* outer /* inner */ still */ mod a;\nlet s = r#\"mod fake;\"#;\nlet q = '\\'';\nmod real;\n";
        let r = m("rust", src);
        assert!(r.code.contains("&'a str"));
        assert!(r.code.contains("mod a;"));
        assert!(!r.code.contains("still"));
        assert!(!r.blank.contains("mod fake"));
        assert!(r.blank.contains("mod real;"));
    }

    #[test]
    fn python_triple_quotes() {
        let r = m("python", "\"\"\"\nimport fake\n\"\"\"\nimport real  # import nope\n\n");
        assert!(!r.blank.contains("import fake"));
        assert!(r.blank.contains("import real"));
        assert!(!r.code.contains("nope"));
        assert_eq!(r.stats.total, 5);
        assert_eq!(r.stats.blank, 1);
    }

    #[test]
    fn unterminated_string_stops_at_newline() {
        let r = m("javascript", "const x = 'oops\nimport y from 'y';\n");
        assert!(r.blank.contains("import y from"));
    }

    #[test]
    fn line_index() {
        let s = "a\nbc\n\nd";
        let li = LineIndex::new(s);
        assert_eq!(li.line_of(0), 1);
        assert_eq!(li.line_of(2), 2);
        assert_eq!(li.line_of(5), 3);
        assert_eq!(li.line_of(6), 4);
    }
}
