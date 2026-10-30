mod common;
use common::Fixture;
use std::process::{Command, Output};

fn chronicle(args: &[&str], dir: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_git-chronicle"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}
fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}
fn code(o: &Output) -> i32 {
    o.status.code().unwrap()
}

fn cyc_repo(resolve: bool) -> Fixture {
    let mut f = Fixture::new();
    f.write("src/a.ts", "import './b'\nexport const a = 1\n");
    f.write("src/b.ts", "export const b = 1\n");
    f.commit("Alice", "c1");
    f.write("src/c.ts", "import './a'\n");
    f.commit("Alice", "c2");
    f.write("src/b.ts", "import './c'\nexport const b = 1\n");
    f.commit("Bob", "c3 close the loop");
    if resolve {
        f.write("src/b.ts", "export const b = 1\n");
        f.commit("Alice", "c4");
    }
    f
}

fn silo_repo() -> Fixture {
    let mut f = Fixture::new();
    f.write("src/hub.ts", "export const h = 1\n");
    for i in 0..5 {
        f.write(&format!("src/u{i}.ts"), "import './hub'\n");
    }
    f.commit("Bob", "all bob");
    f
}

#[test]
fn audit_reports_cycles_and_fail_flag_sets_exit_1() {
    let f = cyc_repo(false);
    let o = chronicle(&["audit"], f.path());
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(
        out(&o).contains("src/a.ts") && out(&o).contains("c3 close the loop"),
        "{}",
        out(&o)
    );
    assert_eq!(code(&chronicle(&["audit", "--fail-on-cycles"], f.path())), 1);
    assert_eq!(
        code(&chronicle(&["audit", "--fail-on-cycles"], cyc_repo(true).path())),
        0
    );
}

#[test]
fn audit_json_and_markdown_and_sarif_formats() {
    let f = cyc_repo(false);
    let j: serde_json::Value =
        serde_json::from_str(&out(&chronicle(&["audit", "--format", "json"], f.path()))).unwrap();
    assert_eq!(j["cycles"].as_array().unwrap().len(), 1);
    let md = out(&chronicle(&["audit", "--format", "markdown"], f.path()));
    assert!(
        md.contains("## Circular dependencies") && md.contains("`src/b.ts`"),
        "{md}"
    );
    let s: serde_json::Value =
        serde_json::from_str(&out(&chronicle(&["audit", "--format", "sarif"], f.path()))).unwrap();
    assert_eq!(s["version"], "2.1.0");
    let r = &s["runs"][0]["results"][0];
    assert_eq!(r["ruleId"], "chronicle/cycle");
    assert!(
        r["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .unwrap()
            .starts_with("src/")
    );
    assert_eq!(code(&chronicle(&["audit", "--format", "xml"], f.path())), 2);
}

#[test]
fn audit_forbidden_edges_from_config() {
    let f = cyc_repo(false);
    assert_eq!(code(&chronicle(&["audit", "--fail-on-forbidden"], f.path())), 0);
    f.write(
        ".chroniclerc.toml",
        "[architecture.boundaries]\nforbidden_edges = [{ from = \"src/a.ts\", to = \"src/b.ts\" }]\n",
    );
    let o = chronicle(&["audit", "--fail-on-forbidden"], f.path());
    assert_eq!(code(&o), 1);
    assert!(out(&o).contains("src/a.ts -> src/b.ts"), "{}", out(&o));
}

#[test]
fn audit_max_bus_factor_risk_threshold() {
    let f = silo_repo();
    assert_eq!(
        code(&chronicle(&["audit", "--max-bus-factor-risk", "0"], f.path())),
        1
    );
    assert_eq!(
        code(&chronicle(&["audit", "--max-bus-factor-risk", "1"], f.path())),
        0
    );
    assert!(out(&chronicle(&["audit"], f.path())).contains("src/hub.ts"));
}

#[test]
fn blame_edge_finds_the_introducing_commit() {
    let f = cyc_repo(true);
    let o = chronicle(&["blame-edge", "src/b.ts", "src/c.ts"], f.path());
    assert_eq!(code(&o), 0, "{}", err(&o));
    let s = out(&o);
    assert!(
        s.contains("Edge Introduced in Commit:") && s.contains("(2023-11-17)"),
        "{s}"
    );
    assert!(s.contains("Author: Bob <bob@example.com>"), "{s}");
    assert!(s.contains("\"c3 close the loop\""), "{s}");
    assert!(s.contains("closed an import cycle"), "{s}");
    assert!(s.contains("no longer present"), "{s}");
}

#[test]
fn blame_edge_unknown_edge_is_a_clean_error() {
    let f = cyc_repo(false);
    let o = chronicle(&["blame-edge", "src/a.ts", "src/c.ts"], f.path());
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("no import edge"), "{}", err(&o));
}

fn pr_repo() -> Fixture {
    let mut f = cyc_repo(false);
    // rewind: build base on main without the cycle-closing commit
    f.git(&["reset", "-q", "--hard", "HEAD~1"]);
    f.git(&["checkout", "-q", "-b", "feature"]);
    f.write("src/b.ts", "import './c'\nexport const b = 1\n");
    f.write("src/new.ts", "import './a'\n");
    f.commit("Bob", "feature work");
    f
}

#[test]
fn pr_diff_markdown_lists_new_modules_edges_and_cycles() {
    let f = pr_repo();
    let o = chronicle(&["pr-diff", "main", "feature"], f.path());
    assert_eq!(code(&o), 0, "{}", err(&o));
    let s = out(&o);
    assert!(s.contains("+2 edges, -0 edges"), "{s}");
    assert!(s.contains("`src/new.ts`"), "{s}");
    assert!(
        s.contains("New circular dependencies") && s.contains("src/a.ts"),
        "{s}"
    );
    assert_eq!(
        code(&chronicle(
            &["pr-diff", "main", "feature", "--fail-on-cycles"],
            f.path()
        )),
        1
    );
}

#[test]
fn pr_diff_of_identical_refs_reports_no_changes() {
    let f = pr_repo();
    let o = chronicle(&["pr-diff", "main", "main", "--fail-on-cycles"], f.path());
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("No architectural changes"), "{}", out(&o));
    let j: serde_json::Value = serde_json::from_str(&out(&chronicle(
        &["pr-diff", "main", "feature", "--format", "json"],
        f.path(),
    )))
    .unwrap();
    assert_eq!(j["new_cycles"].as_array().unwrap().len(), 1);
}

#[test]
fn pr_diff_bad_ref_is_a_clean_error() {
    let f = pr_repo();
    let o = chronicle(&["pr-diff", "main", "nope"], f.path());
    assert_eq!(code(&o), 2);
}
