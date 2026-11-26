//! Lexical extractors. Each receives:
//! - `src`: original text (only for constructs living in comments, e.g. GraphQL `#import`)
//! - `code`: comments masked, strings intact (for specifiers)
//! - `blank`: comments and string contents masked (for keywords/names)
//!
//! All three have identical byte offsets.

use super::{FileFacts, ImportKind, ImportRef, Symbol, SymbolKind};
use crate::lang::mask::LineIndex;
use regex::Regex;
use std::sync::LazyLock;

macro_rules! re {
    ($s:expr) => {{
        static R: LazyLock<Regex> = LazyLock::new(|| Regex::new($s).expect("valid regex"));
        &*R
    }};
}

fn imp(f: &mut FileFacts, li: &LineIndex, at: usize, spec: &str, kind: ImportKind) {
    let spec = spec.trim();
    if spec.is_empty() || spec.len() > 512 {
        return;
    }
    f.imports.push(ImportRef { spec: spec.to_string(), line: li.line_of(at), kind, names: Vec::new() });
}

fn sym(f: &mut FileFacts, li: &LineIndex, at: usize, name: &str, kind: SymbolKind, exported: bool) {
    if name.is_empty() || f.symbols.len() >= 5000 {
        return;
    }
    f.symbols.push(Symbol { name: name.to_string(), kind, line: li.line_of(at), exported });
}

/// True when the keyword at `at` is real code (not inside a string literal).
fn is_code(blank: &str, at: usize, kw: &str) -> bool {
    blank.get(at..at + kw.len()) == Some(kw)
}

fn prev_char(s: &str, at: usize) -> Option<char> {
    s[..at].chars().next_back()
}

// ---------------------------------------------------------------------------------------------
// Complexity indicators (estimated)

pub fn decision_points(blank: &str) -> u32 {
    re!(r"\b(?:if|elif|elsif|for|foreach|while|case|catch|except|when|guard|unless|until)\b|&&|\|\||\?\?")
        .find_iter(blank)
        .count()
        .min(u32::MAX as usize) as u32
}

pub fn brace_depth(blank: &str) -> u32 {
    let (mut d, mut max) = (0i32, 0i32);
    for c in blank.chars() {
        match c {
            '{' => {
                d += 1;
                max = max.max(d);
            }
            '}' => d = (d - 1).max(0),
            _ => {}
        }
    }
    max as u32
}

pub fn indent_depth(blank: &str) -> u32 {
    blank
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut cols = 0;
            for c in l.chars() {
                match c {
                    ' ' => cols += 1,
                    '\t' => cols += 4,
                    _ => break,
                }
            }
            cols / 4
        })
        .max()
        .unwrap_or(0)
}

/// Keep only `<script>` block contents (same byte offsets, everything else blanked).
pub fn script_only(src: &str) -> String {
    let mut out: Vec<u8> = src.bytes().map(|b| if b == b'\n' || b == b'\r' { b } else { b' ' }).collect();
    let open = re!(r"(?i)<script\b[^>]*>");
    let close = re!(r"(?i)</script\s*>");
    let mut pos = 0;
    while let Some(m) = open.find_at(src, pos) {
        let start = m.end();
        let end = close.find_at(src, start).map(|c| c.start()).unwrap_or(src.len());
        out[start..end].copy_from_slice(&src.as_bytes()[start..end]);
        pos = end;
    }
    // Only whole UTF-8 sequences were copied (boundaries are ASCII '<' / '>'), so this is valid.
    String::from_utf8(out).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// JavaScript / TypeScript / JSX / TSX (+ Vue/Svelte script blocks)

pub fn js(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"\bimport\s+(?:type\s+)?(?:[\w$*{}\s,]+?\s+from\s*)?["']([^"'\n]+)["']"#).captures_iter(code) {
        let m = c.get(0).unwrap();
        if is_code(blank, m.start(), "import") && prev_char(code, m.start()) != Some('.') {
            imp(f, li, m.start(), &c[1], ImportKind::Import);
        }
    }
    for c in re!(r#"\bexport\s+(?:type\s+)?(?:\*(?:\s+as\s+[\w$]+)?|\{[^}]*\})\s*from\s*["']([^"'\n]+)["']"#)
        .captures_iter(code)
    {
        let m = c.get(0).unwrap();
        if is_code(blank, m.start(), "export") {
            imp(f, li, m.start(), &c[1], ImportKind::Reexport);
        }
    }
    for c in re!(r#"\brequire\s*\(\s*["']([^"'\n]+)["']\s*\)"#).captures_iter(code) {
        let m = c.get(0).unwrap();
        if is_code(blank, m.start(), "require") && prev_char(code, m.start()) != Some('.') {
            imp(f, li, m.start(), &c[1], ImportKind::Require);
        }
    }
    for c in re!(r#"\bimport\s*\(\s*["']([^"'\n]+)["']\s*\)"#).captures_iter(code) {
        let m = c.get(0).unwrap();
        if is_code(blank, m.start(), "import") && prev_char(code, m.start()) != Some('.') {
            imp(f, li, m.start(), &c[1], ImportKind::Dynamic);
        }
    }

    // Exports
    for c in re!(r"\bexport\s+(?:declare\s+)?(?:default\s+)?(?:abstract\s+)?(?:async\s+)?(function\*?|class|const|let|var|interface|type|enum|namespace)\s*\*?\s*([A-Za-z_$][\w$]*)").captures_iter(blank) {
        f.exports.push(c[2].to_string());
    }
    if re!(r"\bexport\s+default\b").is_match(blank) {
        f.exports.push("default".into());
    }
    for c in re!(r"\bexport\s+(?:type\s+)?\{([^}]*)\}").captures_iter(blank) {
        let after = &blank[c.get(0).unwrap().end()..];
        if re!(r"^\s*from\b").is_match(after) {
            continue; // re-export, counted as import
        }
        for part in c[1].split(',') {
            let part = part.trim().trim_start_matches("type ").trim();
            let name = part.rsplit(" as ").next().unwrap_or(part).trim();
            if !name.is_empty() {
                f.exports.push(name.to_string());
            }
        }
    }
    if re!(r"\bmodule\.exports\s*=").is_match(blank) {
        f.exports.push("module.exports".into());
    }
    for c in re!(r"\bexports\.([A-Za-z_$][\w$]*)\s*=").captures_iter(blank) {
        f.exports.push(c[1].to_string());
    }
    f.exports.sort();
    f.exports.dedup();

    // Symbols
    let exported = |name: &str, f: &FileFacts| f.exports.iter().any(|e| e == name);
    let mut found: Vec<(usize, String, SymbolKind)> = Vec::new();
    for c in re!(r"\bfunction\s*\*?\s*([A-Za-z_$][\w$]*)\s*\(").captures_iter(blank) {
        found.push((c.get(1).unwrap().start(), c[1].to_string(), SymbolKind::Function));
    }
    for c in re!(r"\bclass\s+([A-Za-z_$][\w$]*)").captures_iter(blank) {
        found.push((c.get(1).unwrap().start(), c[1].to_string(), SymbolKind::Class));
    }
    for c in re!(r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=;\n]+)?=\s*(?:async\s+)?(?:function\b|\([^()]*\)\s*(?::[^=;\n]+)?=>|[A-Za-z_$][\w$]*\s*=>)").captures_iter(blank) {
        found.push((c.get(1).unwrap().start(), c[1].to_string(), SymbolKind::Function));
    }
    for c in re!(r"\binterface\s+([A-Za-z_$][\w$]*)").captures_iter(blank) {
        found.push((c.get(1).unwrap().start(), c[1].to_string(), SymbolKind::Interface));
    }
    for c in re!(r"\btype\s+([A-Za-z_$][\w$]*)\s*(?:<[^>=]*>)?\s*=").captures_iter(blank) {
        found.push((c.get(1).unwrap().start(), c[1].to_string(), SymbolKind::Type));
    }
    for c in re!(r"\benum\s+([A-Za-z_$][\w$]*)\s*\{").captures_iter(blank) {
        found.push((c.get(1).unwrap().start(), c[1].to_string(), SymbolKind::Enum));
    }
    for c in re!(r"\bexport\s+(?:const|let|var)\s+([A-Za-z_$][\w$]*)").captures_iter(blank) {
        let at = c.get(1).unwrap().start();
        if !found.iter().any(|(p, _, _)| *p == at) {
            found.push((at, c[1].to_string(), SymbolKind::Variable));
        }
    }
    found.sort_by_key(|x| x.0);
    found.dedup_by_key(|x| x.0);
    for (at, name, kind) in found {
        let e = exported(&name, f);
        sym(f, li, at, &name, kind, e);
    }
}

// ---------------------------------------------------------------------------------------------
// CSS / SCSS / Less

pub fn css(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"@(?:import|use|forward)\s+(?:url\(\s*)?["']?([^"')\s;]+)"#).captures_iter(code) {
        let m = c.get(0).unwrap();
        if !is_code(blank, m.start(), "@") {
            continue;
        }
        let spec = &c[1];
        if spec.starts_with("http:")
            || spec.starts_with("https:")
            || spec.starts_with("//")
            || spec.starts_with("data:")
        {
            continue;
        }
        imp(f, li, m.start(), spec, ImportKind::Stylesheet);
    }
    for c in re!(r"@(mixin|function)\s+([\w-]+)").captures_iter(blank) {
        let m = c.get(2).unwrap();
        sym(f, li, m.start(), &c[2], SymbolKind::Function, true);
    }
}

// ---------------------------------------------------------------------------------------------
// HTML

pub fn html(src: &str, code: &str, _blank: &str, li: &LineIndex, f: &mut FileFacts) {
    let local = |s: &str| {
        !(s.contains("://")
            || s.starts_with("//")
            || s.starts_with("data:")
            || s.starts_with('#')
            || s.starts_with("mailto:"))
    };
    for c in re!(r#"(?i)<script\b[^>]*\bsrc\s*=\s*["']([^"']+)["']"#).captures_iter(code) {
        if local(&c[1]) {
            imp(f, li, c.get(0).unwrap().start(), &c[1], ImportKind::Script);
        }
    }
    for c in re!(r#"(?i)<link\b[^>]*>"#).find_iter(code) {
        let tag = c.as_str();
        let rel = re!(r#"(?i)\brel\s*=\s*["']([^"']+)["']"#)
            .captures(tag)
            .map(|r| r[1].to_ascii_lowercase())
            .unwrap_or_default();
        if !(rel.contains("stylesheet") || rel.contains("modulepreload")) {
            continue;
        }
        if let Some(h) = re!(r#"(?i)\bhref\s*=\s*["']([^"']+)["']"#).captures(tag) {
            if local(&h[1]) {
                imp(f, li, c.start(), &h[1], ImportKind::Stylesheet);
            }
        }
    }
    // Inline module scripts.
    let inline = script_only(src);
    let m = crate::lang::mask::mask(&inline, &crate::lang::by_id("javascript").unwrap().syntax);
    let before = f.imports.len();
    let mut tmp = FileFacts::default();
    js(&inline, &m.code, &m.blank, li, &mut tmp);
    f.imports.extend(tmp.imports);
    let _ = before;
}

// ---------------------------------------------------------------------------------------------
// Rust

/// Flatten a `use` tree (`a::{b, c::{d, e}}`) into paths.
pub fn flatten_use(tree: &str) -> Vec<String> {
    fn rec(prefix: &str, t: &str, out: &mut Vec<String>) {
        let t = t.trim();
        if let Some(open) = t.find('{') {
            let head = t[..open].trim().trim_end_matches("::");
            let close = t.rfind('}').unwrap_or(t.len());
            let inner = &t[open + 1..close];
            let base = join(prefix, head);
            // split on top-level commas
            let (mut depth, mut start) = (0, 0);
            for (i, ch) in inner.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        rec(&base, &inner[start..i], out);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            rec(&base, &inner[start..], out);
        } else {
            let t = t.split(" as ").next().unwrap_or(t).trim();
            if t.is_empty() {
                return;
            }
            if t == "self" {
                out.push(prefix.to_string());
            } else {
                out.push(join(prefix, t));
            }
        }
    }
    fn join(a: &str, b: &str) -> String {
        match (a.is_empty(), b.is_empty()) {
            (true, _) => b.to_string(),
            (_, true) => a.to_string(),
            _ => format!("{a}::{b}"),
        }
    }
    let compact: String = tree.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = Vec::new();
    rec("", &compact, &mut out);
    out.retain(|p| !p.is_empty());
    out
}

pub fn rust(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    let path_attr = re!(r#"#\[path\s*=\s*"([^"]+)"\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;"#);
    let mut with_path = std::collections::HashSet::new();
    for c in path_attr.captures_iter(code) {
        let m = c.get(0).unwrap();
        with_path.insert(c.get(2).unwrap().start());
        f.imports.push(ImportRef {
            spec: c[2].to_string(),
            line: li.line_of(m.start()),
            kind: ImportKind::Mod,
            names: vec![format!("path:{}", &c[1])],
        });
    }
    for c in re!(r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+([A-Za-z_]\w*)[ \t]*;").captures_iter(blank) {
        let g = c.get(1).unwrap();
        if !with_path.contains(&g.start()) {
            imp(f, li, g.start(), &c[1], ImportKind::Mod);
        }
    }
    // Inline `mod name { ... }` blocks: `super::` inside them refers to the file's own module.
    let mut inline_ranges: Vec<(usize, usize)> = Vec::new();
    for m in re!(r"\bmod[ \t]+[A-Za-z_]\w*[ \t]*\{").find_iter(blank) {
        let open = m.end() - 1;
        let mut depth = 0i32;
        let mut end = blank.len();
        for (i, b) in blank.as_bytes()[open..].iter().enumerate() {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        inline_ranges.push((open, end));
    }
    for c in re!(r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?use[ \t]+([^;]+);").captures_iter(blank) {
        let g = c.get(1).unwrap();
        let depth = inline_ranges.iter().filter(|(s, e)| *s < g.start() && g.start() < *e).count();
        for p in flatten_use(&c[1]) {
            let names = if depth > 0 { vec![format!("inline:{depth}")] } else { Vec::new() };
            f.imports.push(ImportRef { spec: p, line: li.line_of(g.start()), kind: ImportKind::Use, names });
        }
    }
    for c in re!(r"\bextern[ \t]+crate[ \t]+([A-Za-z_]\w*)").captures_iter(blank) {
        imp(f, li, c.get(1).unwrap().start(), &c[1], ImportKind::Use);
    }
    for c in re!(r#"(?m)^([ \t]*)(pub(?:\([^)]*\))?[ \t]+)?(?:(?:const|async|unsafe|extern(?:[ \t]+"[^"]*")?)[ \t]+)*fn[ \t]+([A-Za-z_]\w*)"#).captures_iter(blank) {
        let kind = if c[1].is_empty() { SymbolKind::Function } else { SymbolKind::Method };
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, c.get(2).is_some());
    }
    for c in re!(r"(?m)^[ \t]*(pub(?:\([^)]*\))?[ \t]+)?(struct|enum|trait|type|union)[ \t]+([A-Za-z_]\w*)")
        .captures_iter(blank)
    {
        let kind = match &c[2] {
            "struct" | "union" => SymbolKind::Struct,
            "enum" => SymbolKind::Enum,
            "trait" => SymbolKind::Trait,
            _ => SymbolKind::Type,
        };
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, c.get(1).is_some());
    }
    for c in re!(r"(?m)^[ \t]*(pub(?:\([^)]*\))?[ \t]+)?(?:const|static)[ \t]+(?:mut[ \t]+)?([A-Z_][A-Z0-9_]*)[ \t]*:")
        .captures_iter(blank)
    {
        sym(f, li, c.get(2).unwrap().start(), &c[2], SymbolKind::Constant, c.get(1).is_some());
    }
    for c in re!(r"\bmacro_rules![ \t]*([A-Za-z_]\w*)").captures_iter(blank) {
        sym(f, li, c.get(1).unwrap().start(), &c[1], SymbolKind::Macro, true);
    }
    for c in re!(r"(?m)^[ \t]*(pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+([A-Za-z_]\w*)[ \t]*\{").captures_iter(blank) {
        sym(f, li, c.get(2).unwrap().start(), &c[2], SymbolKind::Module, c.get(1).is_some());
    }
    f.symbols.sort_by_key(|s| s.line);
    f.exports =
        f.symbols.iter().filter(|s| s.exported && s.kind != SymbolKind::Method).map(|s| s.name.clone()).collect();
    f.inline_tests = re!(r"#\[(?:cfg\(test\)|test|tokio::test)\]").is_match(blank);
}

// ---------------------------------------------------------------------------------------------
// Python

pub fn python(_src: &str, _code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r"(?m)^[ \t]*import[ \t]+([\w.]+(?:[ \t]+as[ \t]+\w+)?(?:[ \t]*,[ \t]*[\w.]+(?:[ \t]+as[ \t]+\w+)?)*)")
        .captures_iter(blank)
    {
        let at = c.get(0).unwrap().start();
        for part in c[1].split(',') {
            let name = part.split_whitespace().next().unwrap_or("");
            imp(f, li, at, name, ImportKind::Import);
        }
    }
    for c in re!(r"(?m)^[ \t]*from[ \t]+(\.*[\w.]*)[ \t]+import[ \t]+(\([^)]*\)|[^\n]+)").captures_iter(blank) {
        let at = c.get(0).unwrap().start();
        let names: Vec<String> = c[2]
            .trim_matches(|ch: char| ch == '(' || ch == ')' || ch.is_whitespace())
            .split(',')
            .filter_map(|p| p.split_whitespace().next())
            .filter(|n| *n != "*" && *n != "\\")
            .map(String::from)
            .collect();
        f.imports.push(ImportRef { spec: c[1].to_string(), line: li.line_of(at), kind: ImportKind::Import, names });
    }
    for c in re!(r"(?m)^([ \t]*)(?:async[ \t]+)?def[ \t]+([A-Za-z_]\w*)").captures_iter(blank) {
        let top = c[1].is_empty();
        sym(
            f,
            li,
            c.get(2).unwrap().start(),
            &c[2],
            if top { SymbolKind::Function } else { SymbolKind::Method },
            top && !c[2].starts_with('_'),
        );
    }
    for c in re!(r"(?m)^([ \t]*)class[ \t]+([A-Za-z_]\w*)").captures_iter(blank) {
        sym(f, li, c.get(2).unwrap().start(), &c[2], SymbolKind::Class, c[1].is_empty() && !c[2].starts_with('_'));
    }
    for c in re!(r"(?m)^([A-Z][A-Z0-9_]+)[ \t]*(?::[^=\n]+)?=[^=]").captures_iter(blank) {
        sym(f, li, c.get(1).unwrap().start(), &c[1], SymbolKind::Constant, true);
    }
    f.symbols.sort_by_key(|s| s.line);
    f.exports = f.symbols.iter().filter(|s| s.exported).map(|s| s.name.clone()).collect();
}

// ---------------------------------------------------------------------------------------------
// Go

pub fn go(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"(?m)^[ \t]*import[ \t]+(?:[\w.]+[ \t]+)?"([^"]+)""#).captures_iter(code) {
        imp(f, li, c.get(0).unwrap().start(), &c[1], ImportKind::Import);
    }
    for block in re!(r"(?s)\bimport[ \t]*\(([^)]*)\)").captures_iter(code) {
        let g = block.get(1).unwrap();
        if !is_code(blank, block.get(0).unwrap().start(), "import") {
            continue;
        }
        for c in re!(r#"(?m)^[ \t]*(?:[\w.]+[ \t]+)?"([^"]+)""#).captures_iter(g.as_str()) {
            imp(f, li, g.start() + c.get(0).unwrap().start(), &c[1], ImportKind::Import);
        }
    }
    if let Some(c) = re!(r"(?m)^[ \t]*package[ \t]+(\w+)").captures(blank) {
        f.namespace = Some(c[1].to_string());
    }
    let exported = |n: &str| n.chars().next().is_some_and(|c| c.is_uppercase());
    for c in re!(r"(?m)^func[ \t]+(\([^)]*\)[ \t]*)?([A-Za-z_]\w*)").captures_iter(blank) {
        let kind = if c.get(1).is_some() { SymbolKind::Method } else { SymbolKind::Function };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, exported(&c[2]));
    }
    for c in re!(r"(?m)^[ \t]*type[ \t]+([A-Za-z_]\w*)(?:\[[^\]]*\])?[ \t]+(struct|interface)?").captures_iter(blank) {
        let kind = match c.get(2).map(|m| m.as_str()) {
            Some("struct") => SymbolKind::Struct,
            Some("interface") => SymbolKind::Interface,
            _ => SymbolKind::Type,
        };
        sym(f, li, c.get(1).unwrap().start(), &c[1], kind, exported(&c[1]));
    }
    f.symbols.sort_by_key(|s| s.line);
    f.exports =
        f.symbols.iter().filter(|s| s.exported && s.kind != SymbolKind::Method).map(|s| s.name.clone()).collect();
}

// ---------------------------------------------------------------------------------------------
// C / C++

const C_KEYWORDS: &[&str] = &[
    "if",
    "while",
    "for",
    "switch",
    "return",
    "sizeof",
    "catch",
    "else",
    "do",
    "case",
    "defined",
    "static_assert",
    "decltype",
    "alignof",
    "new",
    "delete",
];

pub fn c_family(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"(?m)^[ \t]*#[ \t]*include[ \t]*([<"])([^>"\n]+)[>"]"#).captures_iter(code) {
        let kind = if &c[1] == "<" { ImportKind::SystemInclude } else { ImportKind::Include };
        imp(f, li, c.get(0).unwrap().start(), &c[2], kind);
    }
    for c in re!(r"(?m)^[ \t]*(?:template[ \t]*<[^>]*>[ \t]*)?(class|struct|union)[ \t]+(?:[A-Z_][A-Z0-9_]*[ \t]+)?([A-Za-z_]\w*)[ \t]*(?:final[ \t]*)?(?::[^;{]*)?\{").captures_iter(blank) {
        let kind = if &c[1] == "class" { SymbolKind::Class } else { SymbolKind::Struct };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, true);
    }
    for c in re!(r"\benum[ \t]+(?:class[ \t]+|struct[ \t]+)?([A-Za-z_]\w*)[ \t]*(?::[^{;]*)?\{").captures_iter(blank) {
        sym(f, li, c.get(1).unwrap().start(), &c[1], SymbolKind::Enum, true);
    }
    for c in re!(r"\bnamespace[ \t]+([A-Za-z_][\w:]*)[ \t]*\{").captures_iter(blank) {
        sym(f, li, c.get(1).unwrap().start(), &c[1], SymbolKind::Namespace, true);
    }
    for c in
        re!(r"(?m)^[A-Za-z_][\w:<>,*& \t]*?[ \t*&]([A-Za-z_~][\w:~]*)[ \t]*\([^;{)]*\)[ \t\w]*\{").captures_iter(blank)
    {
        let name = &c[1];
        let short = name.rsplit("::").next().unwrap_or(name);
        if C_KEYWORDS.contains(&short) {
            continue;
        }
        let kind = if name.contains("::") { SymbolKind::Method } else { SymbolKind::Function };
        let is_static = c.get(0).unwrap().as_str().trim_start().starts_with("static");
        sym(f, li, c.get(1).unwrap().start(), name, kind, !is_static);
    }
    for c in re!(r"(?m)^[ \t]*#[ \t]*define[ \t]+([A-Za-z_]\w*)").captures_iter(code) {
        sym(f, li, c.get(1).unwrap().start(), &c[1], SymbolKind::Macro, true);
    }
    f.symbols.sort_by_key(|s| s.line);
}

// ---------------------------------------------------------------------------------------------
// C#

pub fn csharp(_src: &str, _code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in
        re!(r"(?m)^[ \t]*(?:global[ \t]+)?using[ \t]+(?:static[ \t]+)?(?:\w+[ \t]*=[ \t]*)?([A-Za-z_][\w.]*)[ \t]*;")
            .captures_iter(blank)
    {
        imp(f, li, c.get(1).unwrap().start(), &c[1], ImportKind::Use);
    }
    if let Some(c) = re!(r"\bnamespace[ \t]+([A-Za-z_][\w.]*)").captures(blank) {
        f.namespace = Some(c[1].to_string());
    }
    for c in re!(r"(?m)^([ \t]*(?:\[[^\]]*\][ \t]*)*(?:(?:public|private|protected|internal|static|abstract|sealed|partial|readonly|file|unsafe|new)[ \t]+)*)(class|interface|struct|enum|record)[ \t]+(?:class[ \t]+|struct[ \t]+)?([A-Za-z_]\w*)").captures_iter(blank) {
        let kind = match &c[2] {
            "class" | "record" => SymbolKind::Class,
            "interface" => SymbolKind::Interface,
            "struct" => SymbolKind::Struct,
            _ => SymbolKind::Enum,
        };
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, c[1].contains("public"));
    }
    for c in re!(r"(?m)^[ \t]*((?:(?:public|private|protected|internal|static|virtual|override|abstract|async|sealed|partial|extern|unsafe|new)[ \t]+)+)[\w<>\[\],.?]+(?:[ \t]*<[^>]*>)?[ \t]+([A-Za-z_]\w*)[ \t]*(?:<[^>]*>)?[ \t]*\(").captures_iter(blank) {
        sym(f, li, c.get(2).unwrap().start(), &c[2], SymbolKind::Method, c[1].contains("public"));
    }
    f.symbols.sort_by_key(|s| s.line);
    f.exports =
        f.symbols.iter().filter(|s| s.exported && s.kind != SymbolKind::Method).map(|s| s.name.clone()).collect();
}

// ---------------------------------------------------------------------------------------------
// Java / Kotlin

pub fn jvm(_src: &str, _code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r"(?m)^[ \t]*import[ \t]+(?:static[ \t]+)?([A-Za-z_]\w*(?:\.\w+)*(?:\.\*)?)").captures_iter(blank) {
        imp(f, li, c.get(1).unwrap().start(), &c[1], ImportKind::Import);
    }
    if let Some(c) = re!(r"(?m)^[ \t]*package[ \t]+([A-Za-z_][\w.]*)").captures(blank) {
        f.namespace = Some(c[1].to_string());
    }
    for c in re!(r"(?m)^([ \t]*(?:@\w+(?:\([^)]*\))?[ \t]+)*(?:(?:public|private|protected|internal|static|final|abstract|sealed|open|data|inner|enum|annotation|value|inline)[ \t]+)*)(class|interface|enum|record|object)[ \t]+([A-Za-z_]\w*)").captures_iter(blank) {
        let kind = match &c[2] {
            "interface" => SymbolKind::Interface,
            "enum" => SymbolKind::Enum,
            _ => SymbolKind::Class,
        };
        let m = &c[1];
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, !(m.contains("private") || m.contains("internal")));
    }
    for c in re!(r"(?m)^([ \t]*)((?:(?:public|private|protected|internal|override|open|suspend|inline|operator|infix|tailrec|abstract)[ \t]+)*)fun[ \t]+(?:<[^>]*>[ \t]*)?(?:[\w.]+\.)?([A-Za-z_]\w*)[ \t]*\(").captures_iter(blank) {
        let kind = if c[1].is_empty() { SymbolKind::Function } else { SymbolKind::Method };
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, !c[2].contains("private"));
    }
    for c in re!(r"(?m)^[ \t]+((?:(?:public|private|protected|static|final|abstract|synchronized|native|default)[ \t]+)+)(?:<[^>]*>[ \t]*)?[\w<>\[\],.?]+[ \t]+([A-Za-z_]\w*)[ \t]*\(").captures_iter(blank) {
        sym(f, li, c.get(2).unwrap().start(), &c[2], SymbolKind::Method, c[1].contains("public"));
    }
    f.symbols.sort_by_key(|s| s.line);
    f.exports = f
        .symbols
        .iter()
        .filter(|s| {
            s.exported
                && matches!(s.kind, SymbolKind::Class | SymbolKind::Interface | SymbolKind::Enum | SymbolKind::Function)
        })
        .map(|s| s.name.clone())
        .collect();
}

// ---------------------------------------------------------------------------------------------
// Swift

pub fn swift(_src: &str, _code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r"(?m)^[ \t]*(?:@\w+[ \t]+)?import[ \t]+(?:(?:typealias|struct|class|enum|protocol|let|var|func)[ \t]+)?([A-Za-z_][\w.]*)").captures_iter(blank) {
        imp(f, li, c.get(1).unwrap().start(), &c[1], ImportKind::Import);
    }
    for c in re!(r"(?m)^[ \t]*((?:(?:public|open|private|fileprivate|internal|final)[ \t]+)*)(class|struct|enum|protocol|extension|actor)[ \t]+([A-Za-z_]\w*)").captures_iter(blank) {
        let kind = match &c[2] {
            "struct" => SymbolKind::Struct,
            "enum" => SymbolKind::Enum,
            "protocol" => SymbolKind::Interface,
            _ => SymbolKind::Class,
        };
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, !c[1].contains("private"));
    }
    for c in re!(r"(?m)^([ \t]*)((?:(?:public|open|private|fileprivate|internal|static|class|override|mutating|final|@\w+)[ \t]+)*)func[ \t]+([A-Za-z_]\w*)").captures_iter(blank) {
        let kind = if c[1].is_empty() { SymbolKind::Function } else { SymbolKind::Method };
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, !c[2].contains("private"));
    }
    f.symbols.sort_by_key(|s| s.line);
}

// ---------------------------------------------------------------------------------------------
// PHP

pub fn php(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in
        re!(r"(?m)^[ \t]*use[ \t]+(?:function[ \t]+|const[ \t]+)?\\?([A-Za-z_][\w\\]*)(?:[ \t]+as[ \t]+\w+)?[ \t]*;")
            .captures_iter(blank)
    {
        imp(f, li, c.get(1).unwrap().start(), &c[1], ImportKind::Use);
    }
    for c in re!(r#"\b(?:require|include)(?:_once)?\b[ \t]*\(?[ \t]*(__DIR__[ \t]*\.[ \t]*)?["']([^"']+)["']"#)
        .captures_iter(code)
    {
        let m = c.get(0).unwrap();
        if !is_code(blank, m.start(), "re") && !is_code(blank, m.start(), "in") {
            continue;
        }
        let spec = c[2].to_string();
        let spec = if c.get(1).is_some() {
            format!(".{}", if spec.starts_with('/') { spec } else { format!("/{spec}") })
        } else {
            spec
        };
        imp(f, li, m.start(), &spec, ImportKind::Require);
    }
    if let Some(c) = re!(r"(?m)^[ \t]*namespace[ \t]+([A-Za-z_][\w\\]*)").captures(blank) {
        f.namespace = Some(c[1].to_string());
    }
    for c in re!(
        r"(?m)^[ \t]*(?:abstract[ \t]+|final[ \t]+|readonly[ \t]+)*(class|interface|trait|enum)[ \t]+([A-Za-z_]\w*)"
    )
    .captures_iter(blank)
    {
        let kind = match &c[1] {
            "interface" => SymbolKind::Interface,
            "trait" => SymbolKind::Trait,
            "enum" => SymbolKind::Enum,
            _ => SymbolKind::Class,
        };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, true);
    }
    for c in re!(r"(?m)^([ \t]*)((?:(?:public|private|protected|static|abstract|final)[ \t]+)*)function[ \t]+&?[ \t]*([A-Za-z_]\w*)").captures_iter(blank) {
        let kind = if c[1].is_empty() { SymbolKind::Function } else { SymbolKind::Method };
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, !c[2].contains("private"));
    }
    f.symbols.sort_by_key(|s| s.line);
}

// ---------------------------------------------------------------------------------------------
// Ruby

pub fn ruby(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"\brequire_relative[ \t]*\(?[ \t]*["']([^"']+)["']"#).captures_iter(code) {
        let m = c.get(0).unwrap();
        if is_code(blank, m.start(), "require_relative") {
            let s = &c[1];
            let spec = if s.starts_with('.') { s.to_string() } else { format!("./{s}") };
            imp(f, li, m.start(), &spec, ImportKind::Require);
        }
    }
    for c in re!(r#"\brequire[ \t]*\(?[ \t]*["']([^"']+)["']"#).captures_iter(code) {
        let m = c.get(0).unwrap();
        if is_code(blank, m.start(), "require") {
            imp(f, li, m.start(), &c[1], ImportKind::Require);
        }
    }
    for c in re!(r"(?m)^[ \t]*(class|module)[ \t]+([A-Z][\w:]*)").captures_iter(blank) {
        let kind = if &c[1] == "class" { SymbolKind::Class } else { SymbolKind::Module };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, true);
    }
    for c in re!(r"(?m)^([ \t]*)def[ \t]+(self\.)?([\w?!=]+)").captures_iter(blank) {
        let kind = if c[1].is_empty() { SymbolKind::Function } else { SymbolKind::Method };
        sym(f, li, c.get(3).unwrap().start(), &c[3], kind, true);
    }
    f.symbols.sort_by_key(|s| s.line);
}

// ---------------------------------------------------------------------------------------------
// Lua

pub fn lua(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"\brequire[ \t]*\(?[ \t]*["']([^"']+)["']"#).captures_iter(code) {
        let m = c.get(0).unwrap();
        if is_code(blank, m.start(), "require") {
            imp(f, li, m.start(), &c[1], ImportKind::Require);
        }
    }
    for c in re!(r"(?m)^[ \t]*(local[ \t]+)?function[ \t]+([\w.:]+)").captures_iter(blank) {
        let kind = if c[2].contains(':') { SymbolKind::Method } else { SymbolKind::Function };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, c.get(1).is_none());
    }
}

// ---------------------------------------------------------------------------------------------
// Zig

pub fn zig(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"@import\(\s*"([^"]+)"\s*\)"#).captures_iter(code) {
        let m = c.get(0).unwrap();
        if is_code(blank, m.start(), "@import") {
            imp(f, li, m.start(), &c[1], ImportKind::Import);
        }
    }
    for c in re!(r"(?m)^[ \t]*(pub[ \t]+)?(?:inline[ \t]+|export[ \t]+|extern[ \t]+)?fn[ \t]+([A-Za-z_]\w*)")
        .captures_iter(blank)
    {
        sym(f, li, c.get(2).unwrap().start(), &c[2], SymbolKind::Function, c.get(1).is_some());
    }
    for c in re!(r"(?m)^[ \t]*(pub[ \t]+)?const[ \t]+([A-Za-z_]\w*)[ \t]*=[ \t]*(?:extern[ \t]+|packed[ \t]+)?(struct|enum|union)").captures_iter(blank) {
        let kind = if &c[3] == "enum" { SymbolKind::Enum } else { SymbolKind::Struct };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, c.get(1).is_some());
    }
    f.symbols.sort_by_key(|s| s.line);
    f.inline_tests = re!(r#"(?m)^[ \t]*test[ \t]+"[^"]*"[ \t]*\{"#).is_match(code);
}

// ---------------------------------------------------------------------------------------------
// Shell / PowerShell

pub fn shell(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"(?m)^[ \t]*(?:source|\.)[ \t]+["']?([^\s"';|&]+)"#).captures_iter(code) {
        imp(f, li, c.get(0).unwrap().start(), &c[1], ImportKind::Source);
    }
    for c in re!(r"(?m)^[ \t]*(?:function[ \t]+([A-Za-z_][\w-]*)|([A-Za-z_][\w-]*)[ \t]*\(\))").captures_iter(blank) {
        let g = c.get(1).or_else(|| c.get(2)).unwrap();
        sym(f, li, g.start(), g.as_str(), SymbolKind::Function, true);
    }
}

pub fn powershell(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"(?m)^[ \t]*\.[ \t]+["']?([^\s"']+\.ps1)"#).captures_iter(code) {
        imp(f, li, c.get(0).unwrap().start(), &c[1], ImportKind::Source);
    }
    for c in re!(r#"(?im)\bImport-Module[ \t]+(?:-Name[ \t]+)?["']?([^\s"';]+)"#).captures_iter(code) {
        imp(f, li, c.get(0).unwrap().start(), &c[1], ImportKind::Source);
    }
    for c in re!(r#"(?im)^[ \t]*using[ \t]+module[ \t]+["']?([^\s"';]+)"#).captures_iter(code) {
        imp(f, li, c.get(0).unwrap().start(), &c[1], ImportKind::Source);
    }
    for c in re!(r"(?im)^[ \t]*(function|filter|class|enum)[ \t]+([\w-]+)").captures_iter(blank) {
        let kind = match c[1].to_ascii_lowercase().as_str() {
            "class" => SymbolKind::Class,
            "enum" => SymbolKind::Enum,
            _ => SymbolKind::Function,
        };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, true);
    }
}

// ---------------------------------------------------------------------------------------------
// SQL, GraphQL, Terraform, Dockerfile

pub fn sql(_src: &str, _code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"(?i)\bcreate[ \t]+(?:or[ \t]+replace[ \t]+)?(?:(?:temp|temporary|unique|materialized)[ \t]+)?(table|view|function|procedure|trigger|index|type|schema)[ \t]+(?:if[ \t]+not[ \t]+exists[ \t]+)?([\w."\[\]`]+)"#).captures_iter(blank) {
        let kind = match c[1].to_ascii_lowercase().as_str() {
            "table" | "view" | "index" => SymbolKind::Table,
            "function" | "procedure" | "trigger" => SymbolKind::Function,
            "schema" => SymbolKind::Namespace,
            _ => SymbolKind::Type,
        };
        let name = c[2].trim_matches(|ch| ch == '"' || ch == '`' || ch == '[' || ch == ']');
        sym(f, li, c.get(2).unwrap().start(), name, kind, true);
    }
}

pub fn graphql(src: &str, _code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"(?m)^#import[ \t]+["']([^"']+)["']"#).captures_iter(src) {
        imp(f, li, c.get(0).unwrap().start(), &c[1], ImportKind::Import);
    }
    for c in re!(r"(?m)^[ \t]*(?:extend[ \t]+)?(type|interface|enum|input|union|scalar|fragment|query|mutation|subscription|directive[ \t]+@)[ \t]*([A-Za-z_]\w*)").captures_iter(blank) {
        let kind = match &c[1] {
            "interface" => SymbolKind::Interface,
            "enum" => SymbolKind::Enum,
            "query" | "mutation" | "subscription" | "fragment" => SymbolKind::Function,
            _ => SymbolKind::Type,
        };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, true);
    }
}

pub fn terraform(_src: &str, code: &str, blank: &str, li: &LineIndex, f: &mut FileFacts) {
    for c in re!(r#"(?m)^[ \t]*source[ \t]*=[ \t]*"([^"]+)""#).captures_iter(code) {
        let s = &c[1];
        let kind = if s.starts_with("./") || s.starts_with("../") { ImportKind::Import } else { ImportKind::External };
        imp(f, li, c.get(0).unwrap().start(), s, kind);
    }
    for c in re!(r#"(?m)^[ \t]*(resource|data)[ \t]+"([^"]+)"[ \t]+"([^"]+)""#).captures_iter(code) {
        let prefix = if &c[1] == "data" { "data." } else { "" };
        sym(f, li, c.get(2).unwrap().start(), &format!("{prefix}{}.{}", &c[2], &c[3]), SymbolKind::Resource, true);
    }
    for c in re!(r#"(?m)^[ \t]*(module|variable|output)[ \t]+"([^"]+)""#).captures_iter(code) {
        let kind = match &c[1] {
            "module" => SymbolKind::Module,
            "variable" => SymbolKind::Variable,
            _ => SymbolKind::Constant,
        };
        sym(f, li, c.get(2).unwrap().start(), &c[2], kind, true);
    }
    let _ = blank;
}

pub fn dockerfile(_src: &str, code: &str, _blank: &str, li: &LineIndex, f: &mut FileFacts) {
    let mut stages = Vec::new();
    for c in re!(r"(?im)^[ \t]*FROM[ \t]+(?:--\S+[ \t]+)*(\S+)(?:[ \t]+AS[ \t]+(\S+))?").captures_iter(code) {
        let image = &c[1];
        if !stages.iter().any(|s: &String| s.eq_ignore_ascii_case(image)) && image != "scratch" {
            imp(f, li, c.get(0).unwrap().start(), image, ImportKind::External);
        }
        if let Some(s) = c.get(2) {
            stages.push(s.as_str().to_string());
            sym(f, li, s.start(), s.as_str(), SymbolKind::Module, true);
        }
    }
}
