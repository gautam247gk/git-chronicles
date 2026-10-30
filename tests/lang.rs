use git_chronicle::lang::{Import, Index, Lang, extract, lang_of, loc, resolve};

fn specs(lang: Lang, src: &str) -> Vec<String> {
    extract(lang, src).into_iter().map(|i| i.spec).collect()
}

fn idx(paths: &[&str]) -> Index {
    Index::new(paths.iter().copied())
}

fn imp(spec: &str, names: &[&str]) -> Import {
    Import {
        spec: spec.into(),
        names: names.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn detects_language_by_extension() {
    assert_eq!(lang_of("a/b.ts"), Some(Lang::Ts));
    assert_eq!(lang_of("a/b.jsx"), Some(Lang::Ts));
    assert_eq!(lang_of("a/b.py"), Some(Lang::Py));
    assert_eq!(lang_of("a/b.rs"), Some(Lang::Rust));
    assert_eq!(lang_of("a/b.go"), None);
    assert_eq!(lang_of("Makefile"), None);
}

#[test]
fn loc_counts_non_blank_lines() {
    assert_eq!(loc("a\n\n  \nb\n"), 2);
    assert_eq!(loc(""), 0);
}

#[test]
fn ts_extracts_all_import_forms() {
    let src = r#"
import a from './a';
import { x,
  y } from "../b";
import './side';
export * from './re';
export { z } from './z';
const c = require('./c');
const d = await import('./d');
// import nope from './commented';
/* import nope2 from './block'; */
import type { T } from './types';
"#;
    assert_eq!(
        specs(Lang::Ts, src),
        ["./a", "../b", "./side", "./re", "./z", "./c", "./d", "./types"]
    );
}

#[test]
fn ts_resolves_relative_with_extensions_and_index() {
    let i = idx(&[
        "src/a.ts",
        "src/b.tsx",
        "src/lib/index.ts",
        "src/c.js",
        "src/util/x.ts",
        "top.ts",
    ]);
    let r = |s: &str| resolve(Lang::Ts, "src/main.ts", &imp(s, &[]), &i);
    assert_eq!(r("./a"), ["src/a.ts"]);
    assert_eq!(r("./b"), ["src/b.tsx"]);
    assert_eq!(r("./lib"), ["src/lib/index.ts"]);
    assert_eq!(r("./c.js"), ["src/c.js"]);
    assert_eq!(
        r("./a.js"),
        ["src/a.ts"],
        "TS ESM style .js specifier maps to .ts"
    );
    assert_eq!(r("../top"), ["top.ts"]);
    assert_eq!(r("./util/../a"), ["src/a.ts"]);
    assert!(r("react").is_empty());
    assert!(r("./missing").is_empty());
}

#[test]
fn python_extracts_import_forms() {
    let src = "import os, sys as s\nimport a.b.c\nfrom pkg.mod import (x,\n  y as z)\nfrom . import sib\nfrom ..up import q\n# import nope\nfrom m import *\n";
    let got = extract(Lang::Py, src);
    let flat: Vec<(String, Vec<String>)> = got.into_iter().map(|i| (i.spec, i.names)).collect();
    assert_eq!(
        flat,
        vec![
            ("os".into(), vec![]),
            ("sys".into(), vec![]),
            ("a.b.c".into(), vec![]),
            ("pkg.mod".into(), vec!["x".into(), "y".into()]),
            (".".into(), vec!["sib".into()]),
            ("..up".into(), vec!["q".into()]),
            ("m".into(), vec![]),
        ]
    );
}

#[test]
fn python_resolves_absolute_relative_and_src_layout() {
    let i = idx(&[
        "src/pkg/__init__.py",
        "src/pkg/mod.py",
        "src/pkg/sub/leaf.py",
        "src/pkg/sib.py",
        "other/mod.py",
    ]);
    let from = "src/pkg/sub/leaf.py";
    let r = |spec: &str, names: &[&str]| resolve(Lang::Py, from, &imp(spec, names), &i);
    assert_eq!(r("pkg.mod", &[]), ["src/pkg/mod.py"]);
    assert_eq!(r("pkg", &["mod"]), ["src/pkg/__init__.py", "src/pkg/mod.py"]);
    assert_eq!(r("..", &["sib"]), ["src/pkg/sib.py"]);
    assert_eq!(r("..mod", &["thing"]), ["src/pkg/mod.py"]);
    assert_eq!(r(".", &["leaf"]), ["src/pkg/sub/leaf.py"]);
    assert!(r("os", &[]).is_empty());
    // ambiguous absolute import: prefer the candidate closest to the importer
    assert_eq!(
        resolve(Lang::Py, "other/x.py", &imp("mod", &[]), &i),
        ["other/mod.py"]
    );
}

#[test]
fn rust_extracts_mod_and_use() {
    let src = "mod a;\npub mod b;\npub(crate) mod c;\nmod inline { }\nuse crate::x::y::Thing;\nuse crate::{p, q::r};\nuse super::s;\nuse self::t::U;\nuse std::io;\n// use crate::nope;\n";
    assert_eq!(
        specs(Lang::Rust, src),
        [
            "@mod:a",
            "@mod:b",
            "@mod:c",
            "crate::x::y::Thing",
            "crate::p",
            "crate::q::r",
            "super::s",
            "self::t::U"
        ]
    );
}

#[test]
fn rust_resolves_mod_crate_super_self() {
    let i = idx(&[
        "src/lib.rs",
        "src/a.rs",
        "src/a/inner.rs",
        "src/b/mod.rs",
        "src/b/deep.rs",
        "src/c.rs",
    ]);
    let r = |from: &str, s: &str| resolve(Lang::Rust, from, &imp(s, &[]), &i);
    assert_eq!(r("src/lib.rs", "@mod:a"), ["src/a.rs"]);
    assert_eq!(r("src/lib.rs", "@mod:b"), ["src/b/mod.rs"]);
    assert_eq!(r("src/a.rs", "@mod:inner"), ["src/a/inner.rs"]);
    assert_eq!(r("src/c.rs", "crate::a::Thing"), ["src/a.rs"]);
    assert_eq!(r("src/c.rs", "crate::a::inner::f"), ["src/a/inner.rs"]);
    assert_eq!(r("src/b/deep.rs", "super::Thing"), ["src/b/mod.rs"]);
    assert_eq!(r("src/b/mod.rs", "self::deep::X"), ["src/b/deep.rs"]);
    assert_eq!(r("src/a/inner.rs", "super::Y"), ["src/a.rs"]);
    assert!(r("src/c.rs", "crate::nothere::X").is_empty());
}
