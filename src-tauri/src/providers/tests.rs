use super::*;

fn specs(f: &FileFacts) -> Vec<&str> {
    f.imports.iter().map(|i| i.spec.as_str()).collect()
}
fn syms(f: &FileFacts) -> Vec<(&str, SymbolKind)> {
    f.symbols.iter().map(|s| (s.name.as_str(), s.kind)).collect()
}

#[test]
fn javascript_imports_exports_symbols() {
    let src = r#"
import React, { useState } from 'react';
import * as path from "node:path";
import type { Foo } from './types';
import {
  a,
  b as c,
} from '../lib/util.js';
import './side-effect.css';
const x = require('./cjs');
const lazy = () => import('./lazy');
export { helper } from './helper';
export * from './all';
// import nope from './commented';
const s = "import fake from './in-string'";
const t = `require('./in-template')`;
foo.import('./method-call');
export default function App() {}
export const VALUE = 1;
export async function load() {}
const handler = async (e) => {};
class Widget extends Base {}
function internal() {}
export { internal as publicName };
interface Props { a: string }
type Id = string;
module.exports = { x };
"#;
    let f = analyze("tsx", src);
    let s = specs(&f);
    for want in
        ["react", "node:path", "./types", "../lib/util.js", "./side-effect.css", "./cjs", "./lazy", "./helper", "./all"]
    {
        assert!(s.contains(&want), "missing {want} in {s:?}");
    }
    for not in ["./commented", "./in-string", "./in-template", "./method-call"] {
        assert!(!s.contains(&not), "false positive {not}");
    }
    let kinds: Vec<_> = f.imports.iter().map(|i| (i.spec.as_str(), i.kind)).collect();
    assert!(kinds.contains(&("./cjs", ImportKind::Require)));
    assert!(kinds.contains(&("./lazy", ImportKind::Dynamic)));
    assert!(kinds.contains(&("./helper", ImportKind::Reexport)));
    let util = f.imports.iter().find(|i| i.spec == "../lib/util.js").unwrap();
    assert_eq!(util.line, 5);
    for e in ["default", "App", "VALUE", "load", "publicName", "module.exports"] {
        assert!(f.exports.contains(&e.to_string()), "missing export {e}: {:?}", f.exports);
    }
    let sy = syms(&f);
    assert!(sy.contains(&("App", SymbolKind::Function)));
    assert!(sy.contains(&("handler", SymbolKind::Function)));
    assert!(sy.contains(&("Widget", SymbolKind::Class)));
    assert!(sy.contains(&("Props", SymbolKind::Interface)));
    assert!(sy.contains(&("Id", SymbolKind::Type)));
    assert!(sy.contains(&("VALUE", SymbolKind::Variable)));
    assert!(f.symbols.iter().find(|s| s.name == "load").unwrap().exported);
    assert!(!f.symbols.iter().find(|s| s.name == "internal").unwrap().exported);
}

#[test]
fn vue_only_reads_script_blocks() {
    let src = "<template>\n  <p>Don't import x from './template'</p>\n</template>\n<script setup>\nimport Child from './Child.vue'\n</script>\n";
    let f = analyze("vue", src);
    assert_eq!(specs(&f), vec!["./Child.vue"]);
    assert_eq!(f.imports[0].line, 5);
    assert_eq!(f.lines.total, 6);
}

#[test]
fn css_and_html() {
    let f = analyze("scss", "@use 'sass:math';\n@import \"./base\";\n@import url(\"theme.css\");\n@import url(https://cdn/x.css);\n/* @import 'nope'; */\n@mixin card { }\n");
    assert_eq!(specs(&f), vec!["sass:math", "./base", "theme.css"]);
    let h = analyze(
        "html",
        "<link rel=\"stylesheet\" href=\"/src/style.css\">\n<link rel=\"icon\" href=\"/favicon.ico\">\n<script type=\"module\" src=\"/src/main.jsx\"></script>\n<script src=\"https://cdn.x/y.js\"></script>\n<!-- <script src=\"old.js\"></script> -->\n<script type=\"module\">import './inline.js'</script>\n",
    );
    assert_eq!(specs(&h), vec!["/src/main.jsx", "/src/style.css", "./inline.js"]);
}

#[test]
fn rust_mod_use_and_symbols() {
    let src = r#"
//! docs mention mod fake;
pub mod scan;
mod lang;
#[cfg(test)]
mod tests;
#[path = "platform/win.rs"]
mod platform;
use crate::scan::{Excluder, explorer::{list_dir, DirItem}};
use super::error::AppResult as R;
pub use self::lang::detect;
use std::collections::HashMap;
extern crate serde;
pub struct Engine<'a> { s: &'a str }
pub(crate) enum Kind { A }
trait Hidden {}
pub const MAX_FILES: usize = 10;
pub fn run() -> char { let s = "use nope::x;"; 'x' }
impl Engine<'_> {
    pub fn go(&self) {}
}
mod inline { }
macro_rules! m { () => {} }
"#;
    let f = analyze("rust", src);
    let s = specs(&f);
    for want in [
        "scan",
        "lang",
        "tests",
        "platform",
        "crate::scan::Excluder",
        "crate::scan::explorer::list_dir",
        "crate::scan::explorer::DirItem",
        "super::error::AppResult",
        "self::lang::detect",
        "std::collections::HashMap",
        "serde",
    ] {
        assert!(s.contains(&want), "missing {want}: {s:?}");
    }
    assert!(!s.contains(&"fake"));
    assert!(!s.iter().any(|x| x.contains("nope")));
    let plat = f.imports.iter().find(|i| i.spec == "platform").unwrap();
    assert_eq!(plat.names, vec!["path:platform/win.rs".to_string()]);
    let sy = syms(&f);
    assert!(sy.contains(&("Engine", SymbolKind::Struct)));
    assert!(sy.contains(&("Kind", SymbolKind::Enum)));
    assert!(sy.contains(&("Hidden", SymbolKind::Trait)));
    assert!(sy.contains(&("MAX_FILES", SymbolKind::Constant)));
    assert!(sy.contains(&("run", SymbolKind::Function)));
    assert!(sy.contains(&("go", SymbolKind::Method)));
    assert!(sy.contains(&("inline", SymbolKind::Module)));
    assert!(sy.contains(&("m", SymbolKind::Macro)));
    assert!(f.exports.contains(&"run".to_string()));
    assert!(!f.exports.contains(&"Hidden".to_string()));
}

#[test]
fn flatten_use_trees() {
    use super::extract::flatten_use;
    assert_eq!(flatten_use("a::b"), vec!["a::b"]);
    assert_eq!(flatten_use("a::{b, c::{d, e as f}, self}"), vec!["a::b", "a::c::d", "a::c::e", "a"]);
    assert_eq!(flatten_use("std::io::{self, Read}"), vec!["std::io", "std::io::Read"]);
}

#[test]
fn python_imports_and_symbols() {
    let src = "\"\"\"Module docstring: import fake\"\"\"\nimport os, sys as system\nimport pkg.sub\nfrom . import models, views as v\nfrom ..core.base import (\n    Base,\n    Mixin,\n)\nfrom .utils import *\n# import commented\nMAX = 3\nclass Service:\n    def handle(self):\n        pass\ndef main():\n    pass\ndef _private():\n    pass\n";
    let f = analyze("python", src);
    let s = specs(&f);
    assert_eq!(s, vec!["os", "sys", "pkg.sub", ".", "..core.base", ".utils"]);
    assert_eq!(f.imports[3].names, vec!["models", "views"]);
    assert_eq!(f.imports[4].names, vec!["Base", "Mixin"]);
    assert!(f.imports[5].names.is_empty());
    let sy = syms(&f);
    assert!(sy.contains(&("Service", SymbolKind::Class)));
    assert!(sy.contains(&("handle", SymbolKind::Method)));
    assert!(sy.contains(&("MAX", SymbolKind::Constant)));
    assert!(f.exports.contains(&"main".to_string()));
    assert!(!f.exports.contains(&"_private".to_string()));
    assert!(f.max_nesting >= 2);
}

#[test]
fn go_imports_package_symbols() {
    let src = "package server\n\nimport \"fmt\"\nimport (\n\t\"net/http\"\n\tlog \"github.com/sirupsen/logrus\"\n\t\"example.com/app/internal/store\"\n)\n\ntype Server struct{}\ntype handler interface{}\nfunc New() *Server { return nil }\nfunc (s *Server) Start() {}\n";
    let f = analyze("go", src);
    assert_eq!(specs(&f), vec!["fmt", "net/http", "github.com/sirupsen/logrus", "example.com/app/internal/store"]);
    assert_eq!(f.imports[1].line, 5);
    assert_eq!(f.namespace.as_deref(), Some("server"));
    let sy = syms(&f);
    assert!(sy.contains(&("Server", SymbolKind::Struct)));
    assert!(sy.contains(&("handler", SymbolKind::Interface)));
    assert!(sy.contains(&("Start", SymbolKind::Method)));
    assert!(f.exports.contains(&"New".to_string()));
    assert!(!f.exports.contains(&"handler".to_string()));
}

#[test]
fn c_family_includes_and_symbols() {
    let src = "#include <stdio.h>\n#include \"util/math.h\"\n// #include \"old.h\"\n#define MAX 10\nnamespace app {\nclass Engine : public Base {\n};\n}\nstatic int helper(int a) {\n  if (a) { return 1; }\n  return 0;\n}\nint Engine::run() {\n}\nint main(int argc, char** argv) {\n}\n";
    let f = analyze("cpp", src);
    assert_eq!(specs(&f), vec!["stdio.h", "util/math.h"]);
    assert_eq!(f.imports[0].kind, ImportKind::SystemInclude);
    assert_eq!(f.imports[1].kind, ImportKind::Include);
    let sy = syms(&f);
    assert!(sy.contains(&("Engine", SymbolKind::Class)));
    assert!(sy.contains(&("app", SymbolKind::Namespace)));
    assert!(sy.contains(&("main", SymbolKind::Function)));
    assert!(sy.contains(&("Engine::run", SymbolKind::Method)));
    assert!(sy.contains(&("MAX", SymbolKind::Macro)));
    assert!(!f.symbols.iter().find(|s| s.name == "helper").unwrap().exported);
    assert!(!sy.iter().any(|(n, _)| *n == "if"));
}

#[test]
fn csharp_java_kotlin() {
    let cs = analyze("csharp", "using System;\nusing static System.Math;\nusing Alias = App.Core.Models;\nnamespace App.Web.Controllers;\npublic class HomeController : Controller\n{\n    public async Task<IActionResult> Index() { return View(); }\n    private void Helper() {}\n}\ninternal interface IService {}\n");
    assert_eq!(specs(&cs), vec!["System", "System.Math", "App.Core.Models"]);
    assert_eq!(cs.namespace.as_deref(), Some("App.Web.Controllers"));
    let sy = syms(&cs);
    assert!(sy.contains(&("HomeController", SymbolKind::Class)));
    assert!(sy.contains(&("Index", SymbolKind::Method)));
    assert!(sy.contains(&("IService", SymbolKind::Interface)));
    assert!(cs.exports.contains(&"HomeController".to_string()));
    assert!(!cs.exports.contains(&"IService".to_string()));

    let j = analyze("java", "package com.acme.app;\n\nimport java.util.List;\nimport static org.junit.Assert.*;\nimport com.acme.core.Engine;\n\npublic class App {\n    public static void main(String[] args) {}\n}\n");
    assert_eq!(specs(&j), vec!["java.util.List", "org.junit.Assert.*", "com.acme.core.Engine"]);
    assert_eq!(j.namespace.as_deref(), Some("com.acme.app"));
    assert!(syms(&j).contains(&("main", SymbolKind::Method)));

    let k = analyze("kotlin", "package com.acme\nimport kotlinx.coroutines.launch\ndata class User(val name: String)\nobject Registry\nfun main() {}\nprivate fun hidden() {}\n");
    assert_eq!(specs(&k), vec!["kotlinx.coroutines.launch"]);
    let sy = syms(&k);
    assert!(sy.contains(&("User", SymbolKind::Class)));
    assert!(sy.contains(&("Registry", SymbolKind::Class)));
    assert!(sy.contains(&("main", SymbolKind::Function)));
    assert!(!k.exports.contains(&"hidden".to_string()));
}

#[test]
fn php_ruby_lua_zig_swift() {
    let p = analyze("php", "<?php\nnamespace App\\Http;\nuse App\\Models\\User;\nrequire_once __DIR__ . '/helpers.php';\ninclude 'config.php';\nfinal class Controller {\n    public function index() {}\n}\nfunction helper() {}\n");
    assert_eq!(specs(&p), vec!["App\\Models\\User", "./helpers.php", "config.php"]);
    assert_eq!(p.namespace.as_deref(), Some("App\\Http"));
    assert!(syms(&p).contains(&("Controller", SymbolKind::Class)));

    let r = analyze("ruby", "require 'json'\nrequire_relative 'lib/thing'\n# require 'nope'\nmodule Api\n  class Client\n    def call; end\n  end\nend\n");
    assert_eq!(specs(&r), vec!["./lib/thing", "json"]);
    assert!(syms(&r).contains(&("Client", SymbolKind::Class)));

    let l = analyze("lua", "local util = require(\"app.util\")\nlocal json = require 'cjson'\n-- require('nope')\nlocal function helper() end\nfunction M.run() end\nfunction Obj:method() end\n");
    assert_eq!(specs(&l), vec!["app.util", "cjson"]);
    assert!(!l.symbols.iter().find(|s| s.name == "helper").unwrap().exported);

    let z = analyze("zig", "const std = @import(\"std\");\nconst util = @import(\"util.zig\");\npub const Point = struct { x: i32 };\npub fn main() void {}\nfn private() void {}\n");
    assert_eq!(specs(&z), vec!["std", "util.zig"]);
    assert!(syms(&z).contains(&("Point", SymbolKind::Struct)));

    let s = analyze("swift", "import SwiftUI\n@testable import MyApp\nstruct ContentView: View {\n    func body() {}\n}\nprotocol Store {}\n");
    assert_eq!(specs(&s), vec!["SwiftUI", "MyApp"]);
    assert!(syms(&s).contains(&("Store", SymbolKind::Interface)));
}

#[test]
fn scripts_and_infra() {
    let sh = analyze(
        "bash",
        "#!/bin/bash\nsource ./lib/common.sh\n. \"$HOME/.profile\"\nbuild() {\n  echo hi\n}\nfunction deploy {\n}\n",
    );
    assert_eq!(specs(&sh), vec!["./lib/common.sh", "$HOME/.profile"]);
    assert!(syms(&sh).contains(&("build", SymbolKind::Function)));
    assert!(syms(&sh).contains(&("deploy", SymbolKind::Function)));

    let ps = analyze(
        "powershell",
        ". .\\lib\\helpers.ps1\nImport-Module PSReadLine\n<# Import-Module Nope #>\nfunction Get-Thing { }\n",
    );
    assert_eq!(specs(&ps), vec![".\\lib\\helpers.ps1", "PSReadLine"]);
    assert!(syms(&ps).contains(&("Get-Thing", SymbolKind::Function)));

    let sql = analyze("sql", "CREATE TABLE IF NOT EXISTS users (id int);\n-- CREATE TABLE nope (x int);\ncreate or replace view active_users as select 1;\n");
    assert_eq!(syms(&sql), vec![("users", SymbolKind::Table), ("active_users", SymbolKind::Table)]);

    let gql =
        analyze("graphql", "#import \"./fragments.graphql\"\ntype User { id: ID }\nquery GetUser { user { id } }\n");
    assert_eq!(specs(&gql), vec!["./fragments.graphql"]);
    assert!(syms(&gql).contains(&("User", SymbolKind::Type)));

    let tf = analyze("terraform", "module \"vpc\" {\n  source = \"terraform-aws-modules/vpc/aws\"\n}\nmodule \"local\" {\n  source = \"./modules/net\"\n}\nresource \"aws_s3_bucket\" \"logs\" {}\n");
    assert_eq!(specs(&tf), vec!["terraform-aws-modules/vpc/aws", "./modules/net"]);
    assert_eq!(tf.imports[0].kind, ImportKind::External);
    assert!(syms(&tf).contains(&("aws_s3_bucket.logs", SymbolKind::Resource)));

    let d = analyze(
        "dockerfile",
        "FROM node:22-alpine AS build\nRUN npm ci\nFROM build AS test\nFROM --platform=linux/amd64 nginx:1.27\n",
    );
    assert_eq!(specs(&d), vec!["node:22-alpine", "nginx:1.27"]);
}

#[test]
fn complexity_and_nesting_ignore_strings_and_comments() {
    let f = analyze(
        "javascript",
        "function f(a){\n  // if if if\n  const s = 'if while for';\n  if (a && b) { for (;;) { while (x) {} } }\n}\n",
    );
    assert_eq!(f.complexity, 4); // if, &&, for, while
    assert_eq!(f.max_nesting, 4);
    let md = analyze("markdown", "# Title\n\ntext\n");
    assert_eq!(md.complexity, 0);
    assert_eq!(md.lines.total, 3);
    assert_eq!(md.lines.blank, 1);
}

#[test]
fn multibyte_text_keeps_offsets_aligned() {
    let src = "// açıklama — üç\nconst s = 'ğüşiöç'; import x from './x';\n";
    let f = analyze("javascript", src);
    assert_eq!(specs(&f), vec!["./x"]);
    assert_eq!(f.imports[0].line, 2);
}
