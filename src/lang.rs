//! Regex-based import extraction and path resolution for TS/JS, Python and Rust.
//! This is a heuristic scanner, not a parser: see docs/limitations.md.
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ts,
    Py,
    Rust,
}

/// A raw import. `spec` is the module path as written (Python keeps leading dots,
/// Rust `mod foo;` is encoded as `@mod:foo`); `names` are Python `from X import a, b` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub spec: String,
    pub names: Vec<String>,
}

pub fn lang_of(path: &str) -> Option<Lang> {
    match path.rsplit_once('.')?.1 {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => Some(Lang::Ts),
        "py" => Some(Lang::Py),
        "rs" => Some(Lang::Rust),
        _ => None,
    }
}

pub fn loc(text: &str) -> u32 {
    text.lines().filter(|l| !l.trim().is_empty()).count() as u32
}

/// Set of repository files plus a suffix index used for Python absolute imports.
pub struct Index {
    files: HashSet<String>,
    py: HashMap<String, Vec<String>>,
}

impl Index {
    pub fn new<'a>(paths: impl IntoIterator<Item = &'a str>) -> Index {
        let mut files = HashSet::new();
        let mut py: HashMap<String, Vec<String>> = HashMap::new();
        for p in paths {
            files.insert(p.to_string());
            if let Some(stem) = p.strip_suffix(".py") {
                let mut segs: Vec<&str> = stem.split('/').collect();
                if segs.last() == Some(&"__init__") {
                    segs.pop();
                }
                for i in 0..segs.len() {
                    py.entry(segs[i..].join(".")).or_default().push(p.to_string());
                }
            }
        }
        Index { files, py }
    }

    fn has(&self, p: &str) -> bool {
        self.files.contains(p)
    }
}

static BLOCK_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)/\*.*?\*/").unwrap());
static SLASH_LINE_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*//.*$").unwrap());
static HASH_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)#.*$").unwrap());

static TS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r#"(?m)^[ \t]*(?:import|export)\s[^'"`;]*?\bfrom\s*['"]([^'"]+)['"]"#,
        r#"|^[ \t]*import\s*['"]([^'"]+)['"]"#,
        r#"|\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)"#,
        r#"|\brequire\s*\(\s*['"]([^'"]+)['"]\s*\)"#,
    ))
    .unwrap()
});

static PY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?m)^[ \t]*(?:import[ \t]+([^\n]+)",
        r"|from[ \t]+(\.*[\w.]*)[ \t]+import[ \t]+(\([^)]*\)|[^\n]+))"
    ))
    .unwrap()
});

static RS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:mod[ \t]+(\w+)[ \t]*;|use[ \t]+([^;]+);)").unwrap()
});

pub fn extract(lang: Lang, text: &str) -> Vec<Import> {
    match lang {
        Lang::Ts => {
            let t = strip_slash_comments(text);
            TS_RE
                .captures_iter(&t)
                .filter_map(|c| (1..=4).find_map(|i| c.get(i)))
                .map(|m| Import {
                    spec: m.as_str().into(),
                    names: vec![],
                })
                .collect()
        }
        Lang::Py => {
            let t = HASH_COMMENT.replace_all(text, "");
            let mut out = Vec::new();
            for c in PY_RE.captures_iter(&t) {
                if let Some(list) = c.get(1) {
                    for part in list.as_str().split(',') {
                        if let Some(m) = part.split_whitespace().next() {
                            out.push(Import {
                                spec: m.into(),
                                names: vec![],
                            });
                        }
                    }
                } else if let (Some(spec), Some(names)) = (c.get(2), c.get(3)) {
                    let names = names
                        .as_str()
                        .trim_matches(|ch| ch == '(' || ch == ')')
                        .split(',')
                        .filter_map(|n| n.split_whitespace().next())
                        .filter(|n| *n != "*" && *n != "\\")
                        .map(String::from)
                        .collect();
                    out.push(Import {
                        spec: spec.as_str().into(),
                        names,
                    });
                }
            }
            out
        }
        Lang::Rust => {
            let t = strip_slash_comments(text);
            let mut out = Vec::new();
            for c in RS_RE.captures_iter(&t) {
                if let Some(m) = c.get(1) {
                    out.push(Import {
                        spec: format!("@mod:{}", m.as_str()),
                        names: vec![],
                    });
                } else if let Some(u) = c.get(2) {
                    for spec in expand_use(u.as_str()) {
                        if ["crate::", "self::", "super::"]
                            .iter()
                            .any(|p| spec.starts_with(p))
                        {
                            out.push(Import { spec, names: vec![] });
                        }
                    }
                }
            }
            out
        }
    }
}

fn strip_slash_comments(text: &str) -> String {
    let no_block = BLOCK_COMMENT.replace_all(text, "");
    SLASH_LINE_COMMENT.replace_all(&no_block, "").into_owned()
}

/// Expand `a::{b, c::d}` into `a::b`, `a::c::d` (nested groups are truncated at the inner brace).
fn expand_use(raw: &str) -> Vec<String> {
    let s = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s
        .replace(", ", ",")
        .replace("{ ", "{")
        .replace(" }", "}")
        .replace(" ::", "::")
        .replace(":: ", "::");
    let mut out = Vec::new();
    expand(&s, "", &mut out);
    out
}

fn expand(s: &str, prefix: &str, out: &mut Vec<String>) {
    if let Some(open) = s.find('{') {
        let head = format!("{prefix}{}", &s[..open]);
        let close = s.rfind('}').unwrap_or(s.len());
        let inner = &s[open + 1..close.max(open + 1)];
        let (mut depth, mut start) = (0, 0);
        let mut items = Vec::new();
        for (i, ch) in inner.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                ',' if depth == 0 => {
                    items.push(&inner[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        items.push(&inner[start..]);
        for it in items.into_iter().filter(|i| !i.is_empty()) {
            if it.contains('{') {
                // nested group: keep just the path before the brace
                out.push(format!(
                    "{head}{}",
                    it.split('{').next().unwrap().trim_end_matches("::")
                ));
            } else {
                expand(it, &head, out);
            }
        }
    } else {
        let leaf = s.split(" as ").next().unwrap_or(s);
        out.push(format!("{prefix}{leaf}"));
    }
}

pub fn resolve(lang: Lang, from: &str, imp: &Import, idx: &Index) -> Vec<String> {
    let mut out = match lang {
        Lang::Ts => resolve_ts(from, &imp.spec, idx),
        Lang::Py => resolve_py(from, imp, idx),
        Lang::Rust => resolve_rs(from, &imp.spec, idx),
    };
    let mut seen = HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |x| x.0)
}

fn join(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

/// Join and resolve `.`/`..`; `None` if it escapes the repository root.
fn normalize(base: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

const TS_EXTS: [&str; 8] = ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];

fn resolve_ts(from: &str, spec: &str, idx: &Index) -> Vec<String> {
    if !spec.starts_with('.') {
        return vec![]; // bare specifier: external package (tsconfig path aliases unsupported)
    }
    let Some(base) = normalize(dir_of(from), spec) else {
        return vec![];
    };
    let mut cands = vec![base.clone()];
    if let Some((stem, ext)) = base.rsplit_once('.')
        && ["js", "jsx", "mjs", "cjs"].contains(&ext)
    {
        // TS ESM convention: `./a.js` refers to `a.ts`
        cands.extend(["ts", "tsx", "mts", "cts"].iter().map(|e| format!("{stem}.{e}")));
    }
    if !base.is_empty() {
        cands.extend(TS_EXTS.iter().map(|e| format!("{base}.{e}")));
    }
    cands.extend(TS_EXTS.iter().map(|e| join(&base, &format!("index.{e}"))));
    cands.into_iter().find(|c| idx.has(c)).into_iter().collect()
}

fn py_mod(idx: &Index, base: &str) -> Option<String> {
    [format!("{base}.py"), format!("{base}/__init__.py")]
        .into_iter()
        .find(|c| idx.has(c))
}

fn shared_dirs(a: &str, b: &str) -> usize {
    a.split('/').zip(b.split('/')).take_while(|(x, y)| x == y).count()
}

fn py_abs(from: &str, key: &str, idx: &Index) -> Option<String> {
    let c = idx.py.get(key)?;
    c.iter()
        .max_by_key(|p| (shared_dirs(dir_of(from), dir_of(p)), std::cmp::Reverse(p.len())))
        .cloned()
}

fn resolve_py(from: &str, imp: &Import, idx: &Index) -> Vec<String> {
    let dots = imp.spec.chars().take_while(|c| *c == '.').count();
    let rest = &imp.spec[dots..];
    let mut out = Vec::new();
    if dots == 0 {
        out.extend(py_abs(from, rest, idx));
        out.extend(
            imp.names
                .iter()
                .filter_map(|n| py_abs(from, &format!("{rest}.{n}"), idx)),
        );
        return out;
    }
    let mut base = dir_of(from).to_string();
    for _ in 1..dots {
        if base.is_empty() {
            return out;
        }
        base = dir_of(&base).to_string();
    }
    let modbase = if rest.is_empty() {
        base.clone()
    } else {
        join(&base, &rest.replace('.', "/"))
    };
    let names: Vec<String> = imp
        .names
        .iter()
        .filter_map(|n| py_mod(idx, &join(&modbase, n)))
        .collect();
    if rest.is_empty() {
        // bare `from . import x`: x as submodule, else the package itself
        if names.is_empty() {
            let init = join(&base, "__init__.py");
            if idx.has(&init) {
                out.push(init);
            }
        }
    } else {
        out.extend(py_mod(idx, &modbase));
    }
    out.extend(names);
    out
}

fn moddir(from: &str) -> String {
    let file = from.rsplit('/').next().unwrap_or(from);
    let stem = file.strip_suffix(".rs").unwrap_or(file);
    if ["mod", "lib", "main"].contains(&stem) {
        dir_of(from).to_string()
    } else {
        join(dir_of(from), stem)
    }
}

fn crate_root(from: &str) -> String {
    let dirs: Vec<&str> = dir_of(from).split('/').collect();
    match dirs.iter().position(|d| *d == "src") {
        Some(i) => dirs[..=i].join("/"),
        None => String::new(),
    }
}

fn rs_mod(idx: &Index, base: &str) -> Option<String> {
    [format!("{base}.rs"), format!("{base}/mod.rs")]
        .into_iter()
        .find(|c| idx.has(c))
}

fn resolve_rs(from: &str, spec: &str, idx: &Index) -> Vec<String> {
    if let Some(name) = spec.strip_prefix("@mod:") {
        return rs_mod(idx, &join(&moddir(from), name)).into_iter().collect();
    }
    let mut segs: Vec<&str> = spec.split("::").collect();
    let marker = segs.remove(0);
    let mut base = if marker == "crate" {
        crate_root(from)
    } else {
        moddir(from)
    };
    let mut up = usize::from(marker == "super");
    while up > 0 || (marker == "super" && segs.first() == Some(&"super")) {
        if up == 0 {
            segs.remove(0);
        } else {
            up -= 1;
        }
        if base.is_empty() {
            return vec![];
        }
        base = dir_of(&base).to_string();
    }
    while segs.last().is_some_and(|s| *s == "self" || *s == "*") {
        segs.pop();
    }
    for n in (1..=segs.len()).rev() {
        if let Some(p) = rs_mod(idx, &join(&base, &segs[..n].join("/"))) {
            return vec![p];
        }
    }
    if segs.len() <= 1 {
        // item defined directly in the target module
        let hit = if marker == "crate" {
            ["lib.rs", "main.rs"]
                .iter()
                .map(|f| join(&base, f))
                .find(|p| idx.has(p))
        } else {
            rs_mod(idx, &base)
        };
        return hit.into_iter().collect();
    }
    vec![]
}
