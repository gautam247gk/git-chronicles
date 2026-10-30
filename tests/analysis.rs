mod common;
use common::Fixture;
use git_chronicle::analysis::{Analysis, Granularity, Options, analyze};
use git_chronicle::config::Config;
use git_chronicle::git::Repo;

fn run_with(f: &Fixture, g: &str, tweak: impl FnOnce(&mut Options)) -> Analysis {
    let repo = Repo::open(f.path()).unwrap();
    let cfg = Config::load(f.path()).unwrap();
    let mut o = Options {
        granularity: Some(Granularity::parse(g).unwrap()),
        ..Options::default()
    };
    tweak(&mut o);
    analyze(&repo, &cfg, &o).unwrap()
}
fn run(f: &Fixture, g: &str) -> Analysis {
    run_with(f, g, |_| {})
}
fn e(a: &str, b: &str) -> [String; 2] {
    [a.to_string(), b.to_string()]
}

/// c1 Alice: index, a->b. c2 Alice: c->a, lonely. c3 Bob: b->c (closes cycle a,b,c).
/// c4 Alice (optional): b no longer imports c (cycle resolved).
fn cyc_repo(resolve: bool) -> Fixture {
    let mut f = Fixture::new();
    f.write("src/index.ts", "console.log(1)\n");
    f.write("src/a.ts", "import './b'\nexport const a = 1\n");
    f.write("src/b.ts", "export const b = 1\n");
    f.commit("Alice", "c1");
    f.write("src/c.ts", "import './a'\nexport const c = 1\n");
    f.write("src/lonely.ts", "export const l = 1\n");
    f.commit("Alice", "c2");
    f.write("src/b.ts", "import './c'\nexport const b = 1\n");
    f.commit("Bob", "c3 close the loop");
    if resolve {
        f.write("src/b.ts", "export const b = 1\n");
        f.commit("Alice", "c4 break the loop");
    }
    f
}

#[test]
fn one_sample_per_commit_with_exact_deltas() {
    let a = run(&cyc_repo(false), "commit");
    assert_eq!(a.commits.len(), 3);
    assert_eq!(a.graph_deltas.len(), 3);
    assert_eq!(
        a.graph_deltas[0].files_added,
        ["src/a.ts", "src/b.ts", "src/index.ts"]
    );
    assert_eq!(a.graph_deltas[0].edges_added, [e("src/a.ts", "src/b.ts")]);
    assert_eq!(a.graph_deltas[1].files_added, ["src/c.ts", "src/lonely.ts"]);
    assert_eq!(a.graph_deltas[1].edges_added, [e("src/c.ts", "src/a.ts")]);
    assert!(a.graph_deltas[1].cycles_introduced.is_empty());
    assert_eq!(a.graph_deltas[2].edges_added, [e("src/b.ts", "src/c.ts")]);
    assert_eq!(
        a.graph_deltas[2].cycles_introduced,
        [vec!["src/a.ts", "src/b.ts", "src/c.ts"]]
    );
    assert_eq!(a.commits[2].subject, "c3 close the loop");
    assert_eq!(a.commits[2].author, "Bob");
    assert_eq!((a.commits[2].added, a.commits[2].removed), (1, 0));
    assert_eq!(
        (a.commits[2].files, a.commits[2].edges, a.commits[2].cycles),
        (5, 3, 1)
    );
    assert_eq!(a.commits[0].date, "2023-11-15");
}

#[test]
fn head_cycle_names_the_culprit_commit() {
    let a = run(&cyc_repo(false), "commit");
    assert_eq!(a.head.cycles.len(), 1);
    let c = &a.head.cycles[0];
    assert_eq!(c.introduced_sha, a.commits[2].sha);
    assert_eq!(c.subject, "c3 close the loop");
    assert_eq!(c.closing_edges, [e("src/b.ts", "src/c.ts")]);
}

#[test]
fn resolved_cycle_leaves_history_but_not_head() {
    let a = run(&cyc_repo(true), "commit");
    assert_eq!(
        a.commits.iter().map(|c| c.cycles).collect::<Vec<_>>(),
        [0, 0, 1, 0]
    );
    assert!(a.head.cycles.is_empty());
}

#[test]
fn head_metrics_follow_the_spec_formulas() {
    let a = run(&cyc_repo(false), "commit");
    let n = |id: &str| a.head.nodes.iter().find(|n| n.id == id).unwrap();
    let b = n("src/b.ts");
    assert_eq!((b.loc, b.fan_in, b.fan_out, b.churn), (2, 1, 1, 2));
    assert_eq!(b.module, "src");
    assert!(
        (b.author_share - 0.5).abs() < 1e-9,
        "Alice added 1 line, Bob 1: {}",
        b.author_share
    );
    assert!((b.hotspot - 2.0 * 2f64.log10() * 1.0).abs() < 1e-9);
    assert!(n("src/lonely.ts").orphan);
    assert!(!n("src/index.ts").orphan, "index.ts is a conventional entrypoint");
    assert!(!n("src/a.ts").orphan);
}

#[test]
fn silo_needs_dominant_author_and_fan_in_five() {
    let mut f = Fixture::new();
    f.write("src/hub.ts", "export const h = 1\n");
    f.write("src/legacy.ts", "export const l = 1\n");
    for i in 0..5 {
        f.write(&format!("src/user{i}.ts"), "import './hub'\n");
    }
    f.commit("Bob", "bob wrote it all");
    f.advance_days(300);
    f.write("src/more.ts", "import './legacy'\n");
    f.commit("Alice", "alice later");
    let a = run(&f, "commit");
    let n = |id: &str| a.head.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(n("src/hub.ts").fan_in, 5);
    assert!(n("src/hub.ts").silo);
    assert!(n("src/hub.ts").owner_inactive, "Bob has been silent for 300 days");
    assert!(!n("src/legacy.ts").silo, "fan-in 1 < 5");
    assert_eq!(n("src/legacy.ts").top_author, "Bob");
}

#[test]
fn month_granularity_buckets_commits_with_net_delta() {
    let a = run(&cyc_repo(true), "month");
    assert_eq!(a.commits.len(), 1);
    assert_eq!(a.commits[0].commit_count, 4);
    assert_eq!(a.commits[0].subject, "c4 break the loop");
    assert_eq!(a.graph_deltas[0].edges_added.len(), 2);
    assert!(
        a.graph_deltas[0].cycles_introduced.is_empty(),
        "cycle opened and closed inside one bucket"
    );
}

#[test]
fn day_granularity_matches_commit_when_commits_are_a_day_apart() {
    assert_eq!(run(&cyc_repo(true), "day").commits.len(), 4);
}

#[test]
fn tag_granularity_samples_tagged_commits_and_head() {
    let f = cyc_repo(true);
    let sha = Repo::open(f.path()).unwrap().history("HEAD").unwrap()[1]
        .sha
        .clone();
    f.git(&["tag", "v1", &sha]);
    let a = run(&f, "tag");
    assert_eq!(a.commits.len(), 2);
    assert_eq!(a.commits[0].sha, sha);
    assert_eq!(a.commits[0].tags, ["v1"]);
    assert_eq!(a.commits[1].commit_count, 2);
}

#[test]
fn auto_granularity_keeps_small_repos_per_commit() {
    let a = run(&cyc_repo(true), "auto");
    assert_eq!(a.commits.len(), 4);
    assert_eq!(a.granularity, "commit");
}

#[test]
fn since_and_until_window_samples_but_keeps_prior_state() {
    let f = cyc_repo(false);
    let h = Repo::open(f.path()).unwrap().history("HEAD").unwrap();
    let since = format!("@{}", h[1].time);
    let a = run_with(&f, "commit", |o| o.since = Some(since));
    assert_eq!(a.commits.len(), 2);
    assert!(
        a.graph_deltas[0].files_added.contains(&"src/a.ts".to_string()),
        "baseline includes pre-window files"
    );
    let until = format!("@{}", h[1].time);
    let a = run_with(&f, "commit", |o| o.until = Some(until));
    assert_eq!(a.commits.len(), 2);
    assert_eq!(
        a.head.cycles.len(),
        0,
        "head is the last commit inside the window"
    );
}

#[test]
fn polyglot_edges_and_exclude_globs() {
    let mut f = Fixture::new();
    f.write("pkg/__init__.py", "");
    f.write("pkg/a.py", "from . import b\n");
    f.write("pkg/b.py", "X = 1\n");
    f.write("src/lib.rs", "mod x;\n");
    f.write("src/x.rs", "pub fn x() {}\n");
    f.write("README.md", "# hi\n");
    f.commit("Alice", "poly");
    let a = run(&f, "commit");
    assert!(a.head.edges.contains(&e("pkg/a.py", "pkg/b.py")));
    assert!(a.head.edges.contains(&e("src/lib.rs", "src/x.rs")));
    assert!(a.head.nodes.iter().all(|n| n.id != "README.md"));
    let a = run_with(&f, "commit", |o| o.exclude = vec!["pkg/**".into()]);
    assert!(a.head.nodes.iter().all(|n| !n.id.starts_with("pkg/")));
    let a = run_with(&f, "commit", |o| o.paths = vec!["src/**".into()]);
    assert_eq!(a.head.nodes.len(), 2);
}

#[test]
fn default_ignores_skip_node_modules() {
    let mut f = Fixture::new();
    f.write("node_modules/dep/index.js", "module.exports = 1\n");
    f.write("src/a.js", "require('./b')\n");
    f.write("src/b.js", "1\n");
    f.commit("Alice", "x");
    let a = run(&f, "commit");
    assert_eq!(a.head.nodes.len(), 2);
}

#[test]
fn unsupported_only_repo_yields_empty_graph() {
    let mut f = Fixture::new();
    f.write("main.go", "package main\n");
    f.commit("Alice", "go only");
    let a = run(&f, "commit");
    assert!(a.head.nodes.is_empty() && a.head.edges.is_empty());
    assert_eq!(a.commits.len(), 1);
}

#[test]
fn non_utf8_blob_is_skipped_without_error() {
    let mut f = Fixture::new();
    std::fs::write(f.path().join("bin.ts"), [0xff, 0xfe, 0x00, b'i', 0x80]).unwrap();
    f.write("ok.ts", "export {}\n");
    f.commit("Alice", "binary-ish");
    let a = run(&f, "commit");
    assert!(a.head.nodes.iter().any(|n| n.id == "ok.ts"));
}

#[test]
fn self_import_is_not_an_edge_or_cycle() {
    let mut f = Fixture::new();
    f.write("src/self.ts", "import './self'\n");
    f.commit("Alice", "self");
    let a = run(&f, "commit");
    assert!(a.head.edges.is_empty() && a.head.cycles.is_empty());
}

#[test]
fn paths_with_spaces_and_unicode_work() {
    let mut f = Fixture::new();
    f.write("src/my mod.ts", "import './é'\n");
    f.write("src/é.ts", "export {}\n");
    f.commit("Alice", "unicode");
    let a = run(&f, "commit");
    assert_eq!(a.head.edges, [e("src/my mod.ts", "src/é.ts")]);
}

#[test]
fn edge_first_seen_survives_delete_and_readd() {
    let mut f = Fixture::new();
    f.write("a.ts", "import './b'\n");
    f.write("b.ts", "export {}\n");
    f.commit("Alice", "add");
    f.remove("b.ts");
    f.commit("Bob", "delete b");
    f.write("b.ts", "export {}\n");
    f.commit("Carol", "readd b");
    let a = run(&f, "commit");
    assert_eq!(a.edge_first_seen[&("a.ts".to_string(), "b.ts".to_string())], 0);
    assert_eq!(a.commits.iter().map(|c| c.edges).collect::<Vec<_>>(), [1, 0, 1]);
}
