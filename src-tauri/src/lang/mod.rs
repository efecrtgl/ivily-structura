//! Language registry and detection.
//!
//! Detection (which language is this file?) is deliberately separate from analysis (what does
//! it import / declare?). Every language declares its [`Support`] level so the UI never claims
//! more than the engine does.

pub mod mask;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Support {
    /// Recognised by name/extension only.
    Detected,
    /// Imports/exports/symbols extracted by a comment- and string-aware lexical scanner.
    Lexical,
    /// Parser-based semantic analysis.
    Semantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Programming,
    Markup,
    Stylesheet,
    Data,
    Config,
    Documentation,
    Shell,
    Build,
    Query,
}

/// Comment syntax used for masking and comment-line estimation.
#[derive(Debug, Clone, Copy)]
pub struct Syntax {
    pub line: &'static [&'static str],
    pub block: &'static [(&'static str, &'static str)],
    /// String delimiters that may span the scanner (single-char quotes).
    pub quotes: &'static [char],
    /// Python-style triple-quoted strings.
    pub triple_quotes: bool,
    /// Rust-style `'a'` char literals vs `'a` lifetimes.
    pub rust_chars: bool,
    pub nested_block: bool,
}

const C_LIKE: Syntax = Syntax {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: &['"', '\''],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const JS: Syntax = Syntax { quotes: &['"', '\'', '`'], ..C_LIKE };
const GO: Syntax = Syntax { quotes: &['"', '`', '\''], ..C_LIKE };
const RUST: Syntax = Syntax { quotes: &['"'], rust_chars: true, nested_block: true, ..C_LIKE };
const HASH: Syntax = Syntax {
    line: &["#"],
    block: &[],
    quotes: &['"', '\''],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const PY: Syntax = Syntax { triple_quotes: true, ..HASH };
const RUBY: Syntax = Syntax { block: &[("=begin", "=end")], ..HASH };
const PHP: Syntax = Syntax { line: &["//", "#"], ..C_LIKE };
const PS: Syntax = Syntax { block: &[("<#", "#>")], ..HASH };
const LUA: Syntax = Syntax {
    line: &["--"],
    block: &[("--[[", "]]")],
    quotes: &['"', '\''],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const SQL: Syntax = Syntax {
    line: &["--"],
    block: &[("/*", "*/")],
    quotes: &['\''],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const CSS: Syntax = Syntax {
    line: &[],
    block: &[("/*", "*/")],
    quotes: &['"', '\''],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const SCSS: Syntax = Syntax { line: &["//"], ..CSS };
const XML: Syntax = Syntax {
    line: &[],
    block: &[("<!--", "-->")],
    quotes: &[],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const ASM: Syntax = Syntax {
    line: &[";", "#"],
    block: &[],
    quotes: &['"'],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const ZIG: Syntax = Syntax {
    line: &["//"],
    block: &[],
    quotes: &['"', '\''],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const BAT: Syntax = Syntax {
    line: &["REM ", "rem ", "::"],
    block: &[],
    quotes: &['"'],
    triple_quotes: false,
    rust_chars: false,
    nested_block: false,
};
const NONE: Syntax =
    Syntax { line: &[], block: &[], quotes: &[], triple_quotes: false, rust_chars: false, nested_block: false };
const HASKELL: Syntax = Syntax {
    line: &["--"],
    block: &[("{-", "-}")],
    quotes: &['"'],
    triple_quotes: false,
    rust_chars: false,
    nested_block: true,
};

#[derive(Debug, Clone, Copy)]
pub struct Language {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub interpreters: &'static [&'static str],
    pub syntax: Syntax,
    pub support: Support,
}

macro_rules! lang {
    ($id:expr, $name:expr, $cat:ident, $ext:expr, $files:expr, $interp:expr, $syn:expr, $sup:ident) => {
        Language {
            id: $id,
            name: $name,
            category: Category::$cat,
            extensions: $ext,
            filenames: $files,
            interpreters: $interp,
            syntax: $syn,
            support: Support::$sup,
        }
    };
}

/// The registry. Order matters only for documentation; lookups are by extension/filename.
pub static LANGUAGES: &[Language] = &[
    lang!("javascript", "JavaScript", Programming, &["js", "mjs", "cjs"], &[], &["node", "deno", "bun"], JS, Lexical),
    lang!("jsx", "JSX", Programming, &["jsx"], &[], &[], JS, Lexical),
    lang!("typescript", "TypeScript", Programming, &["ts", "mts", "cts"], &[], &["ts-node", "tsx"], JS, Lexical),
    lang!("tsx", "TSX", Programming, &["tsx"], &[], &[], JS, Lexical),
    lang!("vue", "Vue", Programming, &["vue"], &[], &[], JS, Lexical),
    lang!("svelte", "Svelte", Programming, &["svelte"], &[], &[], JS, Lexical),
    lang!("html", "HTML", Markup, &["html", "htm", "xhtml"], &[], &[], XML, Lexical),
    lang!("css", "CSS", Stylesheet, &["css"], &[], &[], CSS, Lexical),
    lang!("scss", "SCSS/Sass", Stylesheet, &["scss", "sass"], &[], &[], SCSS, Lexical),
    lang!("less", "Less", Stylesheet, &["less"], &[], &[], SCSS, Lexical),
    lang!("rust", "Rust", Programming, &["rs"], &[], &[], RUST, Lexical),
    lang!("c", "C", Programming, &["c", "h"], &[], &[], C_LIKE, Lexical),
    lang!(
        "cpp",
        "C++",
        Programming,
        &["cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++", "ipp", "ino"],
        &[],
        &[],
        C_LIKE,
        Lexical
    ),
    lang!("csharp", "C#", Programming, &["cs", "csx"], &[], &[], C_LIKE, Lexical),
    lang!("go", "Go", Programming, &["go"], &[], &[], GO, Lexical),
    lang!("java", "Java", Programming, &["java"], &[], &[], C_LIKE, Lexical),
    lang!("kotlin", "Kotlin", Programming, &["kt", "kts"], &[], &[], C_LIKE, Lexical),
    lang!("swift", "Swift", Programming, &["swift"], &[], &[], C_LIKE, Lexical),
    lang!("python", "Python", Programming, &["py", "pyi", "pyw"], &[], &["python", "python3", "python2"], PY, Lexical),
    lang!("php", "PHP", Programming, &["php", "phtml"], &[], &["php"], PHP, Lexical),
    lang!(
        "ruby",
        "Ruby",
        Programming,
        &["rb", "rake", "gemspec"],
        &["gemfile", "rakefile", "guardfile"],
        &["ruby"],
        RUBY,
        Lexical
    ),
    lang!("lua", "Lua", Programming, &["lua"], &[], &["lua", "luajit"], LUA, Lexical),
    lang!("zig", "Zig", Programming, &["zig", "zon"], &[], &[], ZIG, Lexical),
    lang!("assembly", "Assembly", Programming, &["asm", "s", "nasm", "masm"], &[], &[], ASM, Detected),
    lang!(
        "bash",
        "Shell",
        Shell,
        &["sh", "bash", "zsh", "ksh", "fish"],
        &[".bashrc", ".zshrc", ".profile", ".bash_profile"],
        &["sh", "bash", "zsh", "ksh", "dash"],
        HASH,
        Lexical
    ),
    lang!("powershell", "PowerShell", Shell, &["ps1", "psm1", "psd1"], &[], &["pwsh", "powershell"], PS, Lexical),
    lang!("batch", "Batch", Shell, &["bat", "cmd"], &[], &[], BAT, Detected),
    lang!("sql", "SQL", Query, &["sql", "psql", "mysql"], &[], &[], SQL, Lexical),
    lang!("graphql", "GraphQL", Query, &["graphql", "gql"], &[], &[], HASH, Lexical),
    lang!("markdown", "Markdown", Documentation, &["md", "markdown", "mdx"], &[], &[], NONE, Detected),
    lang!("rst", "reStructuredText", Documentation, &["rst"], &[], &[], NONE, Detected),
    lang!("asciidoc", "AsciiDoc", Documentation, &["adoc", "asciidoc"], &[], &[], NONE, Detected),
    lang!("yaml", "YAML", Data, &["yml", "yaml"], &[], &[], HASH, Detected),
    lang!("toml", "TOML", Data, &["toml"], &[], &[], HASH, Detected),
    lang!(
        "xml",
        "XML",
        Data,
        &["xml", "xsd", "xsl", "xslt", "plist", "csproj", "fsproj", "vbproj", "props", "targets", "resx", "xaml"],
        &[],
        &[],
        XML,
        Detected
    ),
    lang!("json", "JSON", Data, &["json", "jsonc", "json5", "webmanifest"], &[], &[], C_LIKE, Detected),
    lang!(
        "ini",
        "INI / Config",
        Config,
        &["ini", "cfg", "conf", "properties", "editorconfig"],
        &[".editorconfig", ".npmrc", ".gitconfig"],
        &[],
        HASH,
        Detected
    ),
    lang!("dockerfile", "Dockerfile", Build, &["dockerfile"], &["dockerfile", "containerfile"], &[], HASH, Lexical),
    lang!(
        "terraform",
        "Terraform / HCL",
        Config,
        &["tf", "tfvars", "hcl"],
        &[],
        &[],
        Syntax { line: &["#", "//"], ..C_LIKE },
        Lexical
    ),
    lang!("makefile", "Makefile", Build, &["mk", "mak"], &["makefile", "gnumakefile"], &["make"], HASH, Detected),
    lang!("cmake", "CMake", Build, &["cmake"], &["cmakelists.txt"], &[], HASH, Detected),
    lang!("gradle", "Gradle", Build, &["gradle"], &[], &[], C_LIKE, Detected),
    lang!("protobuf", "Protocol Buffers", Data, &["proto"], &[], &[], C_LIKE, Detected),
    lang!("dart", "Dart", Programming, &["dart"], &[], &[], JS, Detected),
    lang!("scala", "Scala", Programming, &["scala", "sc"], &[], &[], C_LIKE, Detected),
    lang!("elixir", "Elixir", Programming, &["ex", "exs"], &[], &["elixir"], HASH, Detected),
    lang!("erlang", "Erlang", Programming, &["erl", "hrl"], &[], &[], Syntax { line: &["%"], ..HASH }, Detected),
    lang!("haskell", "Haskell", Programming, &["hs", "lhs"], &[], &[], HASKELL, Detected),
    lang!("r", "R", Programming, &["r", "rmd"], &[], &["rscript"], HASH, Detected),
    lang!("perl", "Perl", Programming, &["pl", "pm"], &[], &["perl"], HASH, Detected),
    lang!("objc", "Objective-C", Programming, &["m", "mm"], &[], &[], C_LIKE, Detected),
    lang!(
        "fsharp",
        "F#",
        Programming,
        &["fs", "fsi", "fsx"],
        &[],
        &[],
        Syntax { block: &[("(*", "*)")], ..C_LIKE },
        Detected
    ),
    lang!(
        "vb",
        "Visual Basic",
        Programming,
        &["vb", "vbs"],
        &[],
        &[],
        Syntax { line: &["'"], block: &[], quotes: &['"'], ..C_LIKE },
        Detected
    ),
    lang!("solidity", "Solidity", Programming, &["sol"], &[], &[], C_LIKE, Detected),
    lang!("nix", "Nix", Config, &["nix"], &[], &[], HASH, Detected),
    lang!(
        "glsl",
        "Shader (GLSL/HLSL/WGSL)",
        Programming,
        &["glsl", "vert", "frag", "hlsl", "wgsl"],
        &[],
        &[],
        C_LIKE,
        Detected
    ),
    lang!("csv", "CSV/TSV", Data, &["csv", "tsv"], &[], &[], NONE, Detected),
    lang!(
        "text",
        "Plain text",
        Documentation,
        &["txt", "text"],
        &["license", "licence", "authors", "notice", "copying"],
        &[],
        NONE,
        Detected
    ),
];

/// Non-code file kinds recognised for structure/role purposes.
pub fn asset_kind(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "icns" | "svg" | "avif" | "tif" | "tiff" | "psd" => {
            "image"
        }
        "woff" | "woff2" | "ttf" | "otf" | "eot" => "font",
        "mp3" | "wav" | "ogg" | "flac" | "m4a" => "audio",
        "mp4" | "webm" | "mov" | "avi" | "mkv" => "video",
        "pdf" | "docx" | "xlsx" | "pptx" | "odt" => "document",
        "zip" | "gz" | "tgz" | "7z" | "rar" | "tar" | "xz" | "bz2" => "archive",
        "exe" | "dll" | "so" | "dylib" | "lib" | "a" | "o" | "obj" | "pdb" | "class" | "jar" | "wasm" | "node"
        | "pyc" => "binary",
        "db" | "sqlite" | "sqlite3" => "database",
        _ => return None,
    })
}

pub fn by_id(id: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|l| l.id == id)
}

/// Lower-cased extension (last component after the final dot; dotfiles have none).
pub fn extension(file_name: &str) -> Option<String> {
    let name = file_name.trim_start_matches('.');
    let idx = name.rfind('.')?;
    Some(name[idx + 1..].to_ascii_lowercase())
}

/// Detect a language from the file name alone.
pub fn detect(file_name: &str) -> Option<&'static Language> {
    let lower = file_name.to_ascii_lowercase();
    if let Some(l) = LANGUAGES.iter().find(|l| l.filenames.contains(&lower.as_str())) {
        return Some(l);
    }
    // `Dockerfile.dev`, `app.dockerfile`
    if lower.starts_with("dockerfile") || lower.starts_with("containerfile") {
        return by_id("dockerfile");
    }
    let ext = extension(&lower)?;
    LANGUAGES.iter().find(|l| l.extensions.contains(&ext.as_str()))
}

/// Detect from a shebang line (`#!/usr/bin/env python3`).
pub fn detect_shebang(first_line: &str) -> Option<&'static Language> {
    let rest = first_line.strip_prefix("#!")?.trim();
    let mut parts = rest.split_whitespace();
    let mut prog = parts.next()?.rsplit('/').next()?;
    if prog == "env" {
        prog = parts.find(|p| !p.starts_with('-'))?;
    }
    let prog = prog.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    LANGUAGES.iter().find(|l| l.interpreters.iter().any(|i| i.trim_end_matches(|c: char| c.is_ascii_digit()) == prog))
}

/// Registry description for the UI ("what does Structura understand?").
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub extensions: &'static [&'static str],
    pub support: Support,
}

pub fn registry() -> Vec<LanguageInfo> {
    LANGUAGES
        .iter()
        .map(|l| LanguageInfo {
            id: l.id,
            name: l.name,
            category: l.category,
            extensions: l.extensions,
            support: l.support,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_extension_and_name() {
        assert_eq!(detect("main.rs").unwrap().id, "rust");
        assert_eq!(detect("App.JSX").unwrap().id, "jsx");
        assert_eq!(detect("index.d.ts").unwrap().id, "typescript");
        assert_eq!(detect("Dockerfile").unwrap().id, "dockerfile");
        assert_eq!(detect("Dockerfile.prod").unwrap().id, "dockerfile");
        assert_eq!(detect("CMakeLists.txt").unwrap().id, "cmake");
        assert_eq!(detect("Gemfile").unwrap().id, "ruby");
        assert_eq!(detect("main.tf").unwrap().id, "terraform");
        assert_eq!(detect("schema.graphql").unwrap().id, "graphql");
        assert_eq!(detect("build.zig").unwrap().id, "zig");
        assert!(detect("photo.png").is_none());
        assert!(detect("noext").is_none());
        assert!(detect(".gitignore").is_none());
    }

    #[test]
    fn extension_handles_dotfiles() {
        assert_eq!(extension(".eslintrc.json").as_deref(), Some("json"));
        assert_eq!(extension(".gitignore"), None);
        assert_eq!(extension("a.tar.gz").as_deref(), Some("gz"));
    }

    #[test]
    fn detects_shebangs() {
        assert_eq!(detect_shebang("#!/usr/bin/env python3").unwrap().id, "python");
        assert_eq!(detect_shebang("#!/bin/bash").unwrap().id, "bash");
        assert_eq!(detect_shebang("#!/usr/bin/env -S node --flag").unwrap().id, "javascript");
        assert!(detect_shebang("no shebang").is_none());
    }

    #[test]
    fn registry_ids_are_unique_and_extensions_do_not_collide() {
        let mut ids = std::collections::HashSet::new();
        let mut exts = std::collections::HashSet::new();
        for l in LANGUAGES {
            assert!(ids.insert(l.id), "duplicate id {}", l.id);
            for e in l.extensions {
                assert!(exts.insert(*e), "extension {e} registered twice");
            }
        }
    }
}
