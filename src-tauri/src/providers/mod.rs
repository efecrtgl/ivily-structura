//! Language providers: extract imports, exports, symbols and complexity indicators from source.
//!
//! All current providers are **lexical** (see `lang::Support`): they run regular expressions
//! over comment-masked text. They are reliable for conventional code and deliberately ignore
//! constructs they cannot resolve statically (computed `require(x)`, macros, reflection).
//! A provider never invents a relationship: unresolvable specifiers are reported as such.
//!
//! To add a language: implement an `extract_*` function, register it in [`provider_for`], and
//! set the language's `support` to `Lexical` in `lang::LANGUAGES`.

mod extract;

use crate::lang::{self, mask, Support};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImportKind {
    /// ES `import`, Python `import`, Go/Java/Kotlin/Swift `import`.
    Import,
    /// `export ... from`.
    Reexport,
    /// CommonJS `require()`, Ruby `require`, Lua `require`.
    Require,
    /// `import()` / `require_relative` etc. that is still a literal.
    Dynamic,
    /// `#include "x"`.
    Include,
    /// `#include <x>`.
    SystemInclude,
    /// Rust `mod x;`.
    Mod,
    /// Rust `use`, C# `using`, PHP `use`.
    Use,
    /// CSS `@import`/`@use`, HTML `<link>`.
    Stylesheet,
    /// HTML `<script src>`.
    Script,
    /// Shell `source`, PowerShell dot-sourcing / Import-Module.
    Source,
    /// Dockerfile `FROM`, Terraform module `source`.
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRef {
    pub spec: String,
    pub line: u32,
    pub kind: ImportKind,
    /// Imported names where they help resolution (Python `from . import a, b`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Type,
    Constant,
    Variable,
    Module,
    Namespace,
    Macro,
    Resource,
    Table,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line: u32,
    pub exported: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFacts {
    pub imports: Vec<ImportRef>,
    pub exports: Vec<String>,
    pub symbols: Vec<Symbol>,
    /// Declared package/namespace (Java, Kotlin, C#, PHP, Go).
    pub namespace: Option<String>,
    /// Estimated: decision points (`if`, loops, `case`, `catch`, `&&`, `||`, ...).
    pub complexity: u32,
    /// Estimated: maximum block nesting depth.
    pub max_nesting: u32,
    pub lines: mask::LineStats,
}

pub type Extractor = fn(src: &str, code: &str, blank: &str, li: &mask::LineIndex, facts: &mut FileFacts);

/// The extractor for a language id, if one exists.
pub fn provider_for(lang_id: &str) -> Option<Extractor> {
    use extract::*;
    Some(match lang_id {
        "javascript" | "jsx" | "typescript" | "tsx" => js,
        "vue" | "svelte" => js,
        "css" | "scss" | "less" => css,
        "html" => html,
        "rust" => rust,
        "python" => python,
        "go" => go,
        "c" | "cpp" => c_family,
        "csharp" => csharp,
        "java" | "kotlin" => jvm,
        "swift" => swift,
        "php" => php,
        "ruby" => ruby,
        "lua" => lua,
        "zig" => zig,
        "bash" => shell,
        "powershell" => powershell,
        "sql" => sql,
        "graphql" => graphql,
        "terraform" => terraform,
        "dockerfile" => dockerfile,
        _ => return None,
    })
}

/// Line statistics only (for languages without a provider, e.g. Markdown/YAML).
pub fn line_stats(lang_id: Option<&str>, src: &str) -> mask::LineStats {
    match lang_id.and_then(lang::by_id) {
        Some(l) => mask::mask(src, &l.syntax).stats,
        None => mask::mask(src, &lang::by_id("text").unwrap().syntax).stats,
    }
}

/// Full analysis of one file's text.
pub fn analyze(lang_id: &str, src: &str) -> FileFacts {
    let Some(language) = lang::by_id(lang_id) else {
        return FileFacts { lines: line_stats(None, src), ..Default::default() };
    };
    // Single-file components: only the <script> blocks are code; template text (with stray
    // apostrophes) must not confuse the JS masker. Line statistics cover the whole file.
    let sfc = matches!(lang_id, "vue" | "svelte");
    let script;
    let text = if sfc {
        script = extract::script_only(src);
        &script
    } else {
        src
    };
    let m = mask::mask(text, &language.syntax);
    let stats = if sfc { mask::mask(src, &lang::by_id("html").unwrap().syntax).stats } else { m.stats };
    let mut facts = FileFacts { lines: stats, ..Default::default() };
    if language.support >= Support::Lexical {
        if let Some(f) = provider_for(lang_id) {
            let li = mask::LineIndex::new(&m.code);
            f(text, &m.code, &m.blank, &li, &mut facts);
        }
        if language.category == lang::Category::Programming || language.category == lang::Category::Shell {
            facts.complexity = extract::decision_points(&m.blank);
            facts.max_nesting = if matches!(lang_id, "python") {
                extract::indent_depth(&m.blank)
            } else {
                extract::brace_depth(&m.blank)
            };
        }
    }
    facts.imports.dedup_by(|a, b| a.spec == b.spec && a.kind == b.kind && a.line == b.line);
    facts
}

#[cfg(test)]
mod tests;
