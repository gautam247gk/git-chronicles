use git_chronicle::graph::sccs;
use std::collections::BTreeSet;

fn edges(e: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    e.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}
fn nodes(n: &[&str]) -> Vec<String> {
    n.iter().map(|s| s.to_string()).collect()
}

#[test]
fn finds_two_and_three_cycles_sorted() {
    let e = edges(&[
        ("a", "b"),
        ("b", "a"),
        ("x", "y"),
        ("y", "z"),
        ("z", "x"),
        ("a", "x"),
    ]);
    let got = sccs(&nodes(&["a", "b", "x", "y", "z", "q"]), &e);
    assert_eq!(got, vec![vec!["a", "b"], vec!["x", "y", "z"]]);
}

#[test]
fn acyclic_and_self_loops_report_nothing() {
    let e = edges(&[("a", "b"), ("b", "c"), ("c", "c")]);
    assert!(sccs(&nodes(&["a", "b", "c"]), &e).is_empty());
}

#[test]
fn long_chain_does_not_overflow_stack() {
    let n: Vec<String> = (0..50_000).map(|i| format!("n{i}")).collect();
    let mut e: BTreeSet<(String, String)> = (0..49_999)
        .map(|i| (format!("n{i}"), format!("n{}", i + 1)))
        .collect();
    e.insert(("n49999".into(), "n0".into()));
    let got = sccs(&n, &e);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].len(), 50_000);
}
