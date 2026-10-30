mod common;
use common::Fixture;
use git_chronicle::git::Repo;

fn two_commit_repo() -> (Fixture, String, String) {
    let mut f = Fixture::new();
    f.write("src/a.ts", "import './b'\nline2\n");
    f.write("src/b.ts", "export const b = 1\n");
    let c1 = f.commit("Alice", "first");
    f.write("src/a.ts", "import './b'\nline2\nline3\n");
    f.remove("src/b.ts");
    f.write("docs/my file.md", "hello\n");
    let c2 = f.commit("Bob", "second: change & delete");
    (f, c1, c2)
}

#[test]
fn open_rejects_non_repo() {
    let d = tempfile::tempdir().unwrap();
    assert!(Repo::open(d.path()).is_err());
}

#[test]
fn open_rejects_repo_without_commits() {
    let f = Fixture::new();
    let err = Repo::open(f.path()).unwrap_err().to_string();
    assert!(err.contains("no commits"), "{err}");
}

#[test]
fn history_is_chronological_with_changes_and_numstat() {
    let (f, c1, c2) = two_commit_repo();
    let repo = Repo::open(f.path()).unwrap();
    let h = repo.history("HEAD").unwrap();
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].sha.as_str(), h[1].sha.as_str()), (c1.as_str(), c2.as_str()));
    assert_eq!(h[0].author, "Alice");
    assert_eq!(h[1].subject, "second: change & delete");
    assert_eq!(h[0].changes.len(), 2);
    let a = h[1].changes.iter().find(|c| c.path == "src/a.ts").unwrap();
    assert_eq!((a.added, a.removed), (1, 0));
    assert!(a.blob.is_some());
    let b = h[1].changes.iter().find(|c| c.path == "src/b.ts").unwrap();
    assert!(b.blob.is_none(), "deleted file has no blob");
    assert!(h[1].changes.iter().any(|c| c.path == "docs/my file.md"));
}

#[test]
fn blob_reader_returns_content() {
    let (f, _, _) = two_commit_repo();
    let repo = Repo::open(f.path()).unwrap();
    let h = repo.history("HEAD").unwrap();
    let blob = h[0]
        .changes
        .iter()
        .find(|c| c.path == "src/b.ts")
        .unwrap()
        .blob
        .clone()
        .unwrap();
    let mut r = repo.blob_reader().unwrap();
    assert_eq!(r.read(&blob).unwrap().unwrap(), b"export const b = 1\n");
}

#[test]
fn snapshot_lists_blobs_at_ref() {
    let (f, c1, _) = two_commit_repo();
    let repo = Repo::open(f.path()).unwrap();
    let s = repo.snapshot(&c1).unwrap();
    assert_eq!(
        s.keys().cloned().collect::<Vec<_>>(),
        vec!["src/a.ts", "src/b.ts"]
    );
    let head = repo.snapshot("HEAD").unwrap();
    assert!(head.contains_key("docs/my file.md") && !head.contains_key("src/b.ts"));
}

#[test]
fn window_filters_by_git_dates() {
    let (f, c1, c2) = two_commit_repo();
    let repo = Repo::open(f.path()).unwrap();
    let all = repo.window("HEAD", None, None).unwrap();
    assert_eq!(all.len(), 2);
    let until = repo.window("HEAD", None, Some("@1700100000")).unwrap();
    assert!(until.contains(&c1) && !until.contains(&c2));
}

#[test]
fn tags_map_commit_to_names() {
    let (f, c1, _) = two_commit_repo();
    f.git(&["tag", "v1", &c1]);
    f.git(&["tag", "-a", "-m", "rel", "v1.1", &c1]);
    let repo = Repo::open(f.path()).unwrap();
    let t = repo.tags().unwrap();
    let mut names = t[&c1].clone();
    names.sort();
    assert_eq!(names, vec!["v1", "v1.1"]);
}

#[test]
fn changed_paths_between_refs() {
    let (f, c1, c2) = two_commit_repo();
    let repo = Repo::open(f.path()).unwrap();
    let mut p = repo.changed_paths(&c1, &c2).unwrap();
    p.sort();
    assert_eq!(p, vec!["docs/my file.md", "src/a.ts", "src/b.ts"]);
}
