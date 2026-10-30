//! History walk: applies first-parent commits to an in-memory file map, parses each
//! changed blob once (cached by blob SHA), and snapshots the import graph at sample points.
use crate::config::{Config, globset};
use crate::dates::{day_index, fmt_date, month_index, week_index};
use crate::git::{BlobReader, HistCommit, Repo};
use crate::graph::sccs;
use crate::lang::{self, Import, Index, Lang};
use anyhow::{Result, bail};
use globset::GlobSet;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

const DEFAULT_IGNORES: [&str; 5] = [
    "**/node_modules/**",
    "**/dist/**",
    "**/target/**",
    "**/vendor/**",
    "**/*.min.js",
];
const DEFAULT_ENTRYPOINT_NAMES: [&str; 12] = [
    "main.rs",
    "lib.rs",
    "build.rs",
    "index.ts",
    "index.tsx",
    "index.js",
    "index.jsx",
    "main.ts",
    "main.js",
    "__init__.py",
    "__main__.py",
    "setup.py",
];
const MAX_AUTO_SAMPLES: usize = 300;
const MAX_BLOB_BYTES: usize = 2_000_000;
const SILO_SHARE: f64 = 0.8;
const SILO_FAN_IN: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Auto,
    Commit,
    Day,
    Week,
    Month,
    Tag,
}

impl Granularity {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "auto" => Self::Auto,
            "commit" => Self::Commit,
            "day" => Self::Day,
            "week" => Self::Week,
            "month" => Self::Month,
            "tag" => Self::Tag,
            _ => bail!("unknown granularity {s:?} (expected auto|commit|day|week|month|tag)"),
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Commit => "commit",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Tag => "tag",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub rev: String,
    pub since: Option<String>,
    pub until: Option<String>,
    /// `None` means use the config file's `analysis.granularity`.
    pub granularity: Option<Granularity>,
    pub paths: Vec<String>,
    pub exclude: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            rev: "HEAD".into(),
            since: None,
            until: None,
            granularity: None,
            paths: vec![],
            exclude: vec![],
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Sample {
    pub sha: String,
    pub short: String,
    pub author: String,
    #[serde(skip)]
    pub email: String,
    pub time: i64,
    pub date: String,
    pub subject: String,
    pub commit_count: u32,
    pub added: u64,
    pub removed: u64,
    pub files: usize,
    pub loc: u64,
    pub edges: usize,
    pub cycles: usize,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Delta {
    pub files_added: Vec<String>,
    pub files_removed: Vec<String>,
    pub edges_added: Vec<[String; 2]>,
    pub edges_removed: Vec<[String; 2]>,
    pub cycles_introduced: Vec<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct Node {
    pub id: String,
    pub loc: u32,
    pub module: String,
    pub fan_in: u32,
    pub fan_out: u32,
    pub churn: u32,
    pub top_author: String,
    pub author_share: f64,
    pub owner_inactive: bool,
    pub hotspot: f64,
    pub silo: bool,
    pub orphan: bool,
}

#[derive(Debug, Serialize)]
pub struct Cycle {
    pub files: Vec<String>,
    pub introduced_sha: String,
    pub introduced_date: String,
    pub subject: String,
    pub closing_edges: Vec<[String; 2]>,
}

#[derive(Debug, Serialize)]
pub struct Head {
    pub sha: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<[String; 2]>,
    pub cycles: Vec<Cycle>,
}

#[derive(Debug, Serialize)]
pub struct Analysis {
    pub version: u32,
    pub project: Option<String>,
    pub theme: String,
    pub rev: String,
    pub granularity: String,
    pub commits: Vec<Sample>,
    pub graph_deltas: Vec<Delta>,
    pub head: Head,
    /// (importer, imported) -> index of the first sample containing the edge.
    #[serde(skip)]
    pub edge_first_seen: HashMap<(String, String), usize>,
}

struct Parsed {
    loc: u32,
    lang: Lang,
    imports: Vec<Import>,
}

#[derive(Default)]
struct FileStats {
    authors: HashMap<String, u64>,
    touched: Vec<i64>,
}

struct Pending {
    key: i64,
    count: u32,
    added: u64,
    removed: u64,
    last: usize,
}

type Edge = (String, String);

fn bucket(g: Granularity, c: &HistCommit, tags_before: i64) -> i64 {
    match g {
        Granularity::Commit | Granularity::Auto => 0, // handled by caller (each commit is its own bucket)
        Granularity::Day => day_index(c.time),
        Granularity::Week => week_index(c.time),
        Granularity::Month => month_index(c.time),
        Granularity::Tag => tags_before,
    }
}

fn choose_auto(commits: &[&HistCommit]) -> Granularity {
    for g in [Granularity::Commit, Granularity::Day, Granularity::Week] {
        let n = match g {
            Granularity::Commit => commits.len(),
            _ => commits
                .iter()
                .map(|c| bucket(g, c, 0))
                .collect::<HashSet<_>>()
                .len(),
        };
        if n <= MAX_AUTO_SAMPLES {
            return g;
        }
    }
    Granularity::Month
}

fn parse_blob(reader: &mut BlobReader, sha: &str, path: &str) -> Option<Rc<Parsed>> {
    let lang = lang::lang_of(path)?;
    let bytes = reader.read(sha).ok()??;
    let text = if bytes.len() > MAX_BLOB_BYTES {
        None
    } else {
        String::from_utf8(bytes).ok()
    };
    Some(Rc::new(match text {
        Some(t) => Parsed {
            loc: lang::loc(&t),
            lang,
            imports: lang::extract(lang, &t),
        },
        None => Parsed {
            loc: 0,
            lang,
            imports: vec![],
        },
    }))
}

fn edges_of(state: &BTreeMap<String, Rc<Parsed>>) -> BTreeSet<Edge> {
    let idx = Index::new(state.keys().map(String::as_str));
    let mut out = BTreeSet::new();
    for (path, p) in state {
        for imp in &p.imports {
            for target in lang::resolve(p.lang, path, imp, &idx) {
                if &target != path {
                    out.insert((path.clone(), target));
                }
            }
        }
    }
    out
}

struct Filter {
    ignore: GlobSet,
    include: Option<GlobSet>,
}

impl Filter {
    fn new(cfg: &Config, opts: &Options) -> Result<Filter> {
        let mut ignore: Vec<String> = DEFAULT_IGNORES.iter().map(|s| s.to_string()).collect();
        ignore.extend(cfg.project.ignore.iter().cloned());
        ignore.extend(opts.exclude.iter().cloned());
        let include = if opts.paths.is_empty() {
            None
        } else {
            Some(globset(&opts.paths)?)
        };
        Ok(Filter {
            ignore: globset(&ignore)?,
            include,
        })
    }

    fn wanted(&self, p: &str) -> bool {
        lang::lang_of(p).is_some()
            && !self.ignore.is_match(p)
            && self.include.as_ref().is_none_or(|i| i.is_match(p))
    }
}

/// The import graph of one revision (no history needed).
pub struct GraphAt {
    pub files: BTreeSet<String>,
    pub edges: BTreeSet<(String, String)>,
    pub cycles: Vec<Vec<String>>,
}

pub fn graph_at(repo: &Repo, cfg: &Config, opts: &Options, rev: &str) -> Result<GraphAt> {
    let filter = Filter::new(cfg, opts)?;
    let mut reader = repo.blob_reader()?;
    let mut state = BTreeMap::new();
    for (path, sha) in repo.snapshot(rev)? {
        if filter.wanted(&path)
            && let Some(p) = parse_blob(&mut reader, &sha, &path)
        {
            state.insert(path, p);
        }
    }
    let edges = edges_of(&state);
    let files: BTreeSet<String> = state.keys().cloned().collect();
    let cycles = sccs(&files.iter().cloned().collect::<Vec<_>>(), &edges);
    Ok(GraphAt { files, edges, cycles })
}

fn pair(e: &Edge) -> [String; 2] {
    [e.0.clone(), e.1.clone()]
}

fn module_of(path: &str) -> String {
    path.split_once('/').map_or("(root)", |x| x.0).to_string()
}

pub fn analyze(repo: &Repo, cfg: &Config, opts: &Options) -> Result<Analysis> {
    let history = repo.history(&opts.rev)?;
    if history.is_empty() {
        bail!("no commits reachable from {}", opts.rev);
    }
    let windowed = opts.since.is_some() || opts.until.is_some();
    let in_window: Option<HashSet<String>> = if windowed {
        Some(repo.window(&opts.rev, opts.since.as_deref(), opts.until.as_deref())?)
    } else {
        None
    };
    let in_win = |c: &HistCommit| in_window.as_ref().is_none_or(|w| w.contains(&c.sha));
    let tags = repo.tags()?;

    let filter = Filter::new(cfg, opts)?;
    let wanted = |p: &str| filter.wanted(p);

    let window_commits: Vec<&HistCommit> = history.iter().filter(|c| in_win(c)).collect();
    let g = opts
        .granularity
        .unwrap_or(Granularity::parse(&cfg.analysis.granularity)?);
    let g = if g == Granularity::Auto {
        choose_auto(&window_commits)
    } else {
        g
    };

    let mut reader = repo.blob_reader()?;
    let mut cache: HashMap<String, Option<Rc<Parsed>>> = HashMap::new();
    let mut state: BTreeMap<String, Rc<Parsed>> = BTreeMap::new();
    let mut stats: HashMap<String, FileStats> = HashMap::new();
    let mut author_last: HashMap<String, i64> = HashMap::new();

    let mut samples: Vec<Sample> = Vec::new();
    let mut deltas: Vec<Delta> = Vec::new();
    let mut edge_first_seen: HashMap<Edge, usize> = HashMap::new();
    let mut cycle_birth: HashMap<Vec<String>, (usize, Vec<[String; 2]>)> = HashMap::new();
    let mut prev_files: BTreeSet<String> = BTreeSet::new();
    let mut prev_edges: BTreeSet<Edge> = BTreeSet::new();
    let mut prev_cycles: HashSet<Vec<String>> = HashSet::new();
    let mut cur_edges: BTreeSet<Edge> = BTreeSet::new();
    let mut cur_cycles: Vec<Vec<String>> = Vec::new();
    let mut pending: Option<Pending> = None;
    let mut tags_seen: i64 = 0;
    let mut started = false;
    let mut last_applied = 0usize;

    let mut emit = |p: Pending, state: &BTreeMap<String, Rc<Parsed>>| {
        let c = &history[p.last];
        let files: BTreeSet<String> = state.keys().cloned().collect();
        let edges = edges_of(state);
        let nodes: Vec<String> = files.iter().cloned().collect();
        let cycles = sccs(&nodes, &edges);
        let idx = samples.len();
        let added_edges: Vec<[String; 2]> = edges.difference(&prev_edges).map(pair).collect();
        let mut introduced = Vec::new();
        for cy in &cycles {
            if !prev_cycles.contains(cy) {
                let members: HashSet<&String> = cy.iter().collect();
                let closing = added_edges
                    .iter()
                    .filter(|e| members.contains(&e[0]) && members.contains(&e[1]))
                    .cloned()
                    .collect();
                cycle_birth.entry(cy.clone()).or_insert((idx, closing));
                introduced.push(cy.clone());
            }
        }
        for e in &edges {
            edge_first_seen.entry(e.clone()).or_insert(idx);
        }
        deltas.push(Delta {
            files_added: files.difference(&prev_files).cloned().collect(),
            files_removed: prev_files.difference(&files).cloned().collect(),
            edges_added: added_edges,
            edges_removed: prev_edges.difference(&edges).map(pair).collect(),
            cycles_introduced: introduced,
        });
        samples.push(Sample {
            sha: c.sha.clone(),
            short: c.sha.chars().take(7).collect(),
            author: c.author.clone(),
            email: c.email.clone(),
            time: c.time,
            date: fmt_date(c.time),
            subject: c.subject.clone(),
            commit_count: p.count,
            added: p.added,
            removed: p.removed,
            files: files.len(),
            loc: state.values().map(|f| u64::from(f.loc)).sum(),
            edges: edges.len(),
            cycles: cycles.len(),
            tags: tags.get(&c.sha).cloned().unwrap_or_default(),
        });
        prev_cycles = cycles.iter().cloned().collect();
        cur_cycles = cycles;
        prev_files = files;
        cur_edges = edges.clone();
        prev_edges = edges;
    };

    for (i, c) in history.iter().enumerate() {
        let inside = in_win(c);
        if started && !inside {
            break; // past --until
        }
        if inside {
            started = true;
            let key = bucket(g, c, tags_seen);
            let new_bucket = g == Granularity::Commit || pending.as_ref().is_some_and(|p| p.key != key);
            if new_bucket && let Some(p) = pending.take() {
                emit(p, &state);
            }
            let p = pending.get_or_insert(Pending {
                key,
                count: 0,
                added: 0,
                removed: 0,
                last: i,
            });
            p.count += 1;
            p.last = i;
        }
        author_last.insert(c.author.clone(), c.time);
        for ch in c.changes.iter().filter(|ch| wanted(&ch.path)) {
            match &ch.blob {
                None => {
                    state.remove(&ch.path);
                }
                Some(sha) => {
                    let parsed = cache
                        .entry(sha.clone())
                        .or_insert_with(|| parse_blob(&mut reader, sha, &ch.path))
                        .clone();
                    if let Some(p) = parsed {
                        state.insert(ch.path.clone(), p);
                    }
                }
            }
            let s = stats.entry(ch.path.clone()).or_default();
            *s.authors.entry(c.author.clone()).or_default() += ch.added;
            s.touched.push(c.time);
            if inside && let Some(p) = pending.as_mut() {
                p.added += ch.added;
                p.removed += ch.removed;
            }
        }
        if tags.contains_key(&c.sha) {
            tags_seen += 1;
        }
        last_applied = i;
    }
    if let Some(p) = pending.take() {
        emit(p, &state);
    }
    if samples.is_empty() {
        bail!("no commits in the requested --since/--until window");
    }

    // ---- HEAD metrics ----
    let head_c = &history[last_applied];
    let ids: Vec<String> = state.keys().cloned().collect();
    let mut fan_in: HashMap<&str, u32> = HashMap::new();
    let mut fan_out: HashMap<&str, u32> = HashMap::new();
    for (a, b) in &cur_edges {
        *fan_out.entry(a).or_default() += 1;
        *fan_in.entry(b).or_default() += 1;
    }
    let entry_globs = globset(&cfg.project.entrypoints)?;
    let window_start = head_c.time - i64::from(cfg.analysis.hotspot_window_days) * 86_400;
    let inactive_before = head_c.time - i64::from(cfg.analysis.inactive_author_days) * 86_400;
    let nodes: Vec<Node> = ids
        .iter()
        .map(|id| {
            let p = &state[id];
            let (fi, fo) = (
                fan_in.get(id.as_str()).copied().unwrap_or(0),
                fan_out.get(id.as_str()).copied().unwrap_or(0),
            );
            let st = stats.get(id);
            let churn = st.map_or(0, |s| {
                s.touched.iter().filter(|t| **t >= window_start).count() as u32
            });
            let total: u64 = st.map_or(0, |s| s.authors.values().sum());
            let top = st.and_then(|s| s.authors.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))));
            let (top_author, share) = top.map_or((String::new(), 0.0), |(a, n)| {
                (a.clone(), if total == 0 { 0.0 } else { *n as f64 / total as f64 })
            });
            let file = id.rsplit('/').next().unwrap_or(id);
            let conventional = DEFAULT_ENTRYPOINT_NAMES.contains(&file)
                || id.contains("/tests/")
                || id.starts_with("tests/")
                || file.starts_with("test_")
                || file.contains(".test.")
                || file.contains(".spec.")
                || file == "conftest.py";
            Node {
                id: id.clone(),
                loc: p.loc,
                module: module_of(id),
                fan_in: fi,
                fan_out: fo,
                churn,
                owner_inactive: author_last.get(&top_author).is_some_and(|t| *t < inactive_before),
                top_author,
                author_share: share,
                hotspot: f64::from(churn) * f64::from(p.loc.max(1)).log10() * f64::from(fi),
                silo: share > SILO_SHARE && fi >= SILO_FAN_IN,
                orphan: fi == 0 && !conventional && !entry_globs.is_match(id),
            }
        })
        .collect();
    let cycles = cur_cycles
        .iter()
        .map(|files| {
            let (idx, closing) = cycle_birth
                .get(files)
                .cloned()
                .unwrap_or((samples.len() - 1, vec![]));
            Cycle {
                files: files.clone(),
                introduced_sha: samples[idx].sha.clone(),
                introduced_date: samples[idx].date.clone(),
                subject: samples[idx].subject.clone(),
                closing_edges: closing,
            }
        })
        .collect();
    let head = Head {
        sha: head_c.sha.clone(),
        nodes,
        edges: cur_edges.iter().map(pair).collect(),
        cycles,
    };
    Ok(Analysis {
        version: 1,
        project: cfg.project.name.clone(),
        theme: cfg.export.theme.clone(),
        rev: opts.rev.clone(),
        granularity: g.name().into(),
        commits: samples,
        graph_deltas: deltas,
        head,
        edge_first_seen,
    })
}
