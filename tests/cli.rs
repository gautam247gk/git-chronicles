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
fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn repo() -> Fixture {
    let mut f = Fixture::new();
    f.write("src/a.ts", "import './b'\n");
    f.write("src/b.ts", "import './a'\n");
    f.commit("Alice", "cycle from the start");
    f.write("src/c.ts", "export {}\n");
    f.commit("Bob", "more");
    f
}

#[test]
fn export_dash_prints_timeline_json_for_cwd_repo() {
    let f = repo();
    let o = chronicle(&["--export", "-"], f.path());
    assert!(o.status.success(), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["commits"].as_array().unwrap().len(), 2);
    assert_eq!(v["graph_deltas"].as_array().unwrap().len(), 2);
    assert_eq!(
        v["head"]["cycles"][0]["files"],
        serde_json::json!(["src/a.ts", "src/b.ts"])
    );
}

#[test]
fn export_json_file_with_repo_arg_and_filters() {
    let f = repo();
    let out = f.path().join("out.json");
    let o = chronicle(
        &[
            f.path().to_str().unwrap(),
            "--export",
            out.to_str().unwrap(),
            "--path",
            "src/a.ts",
            "-g",
            "month",
        ],
        std::env::temp_dir().as_path(),
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap();
    assert_eq!(v["granularity"], "month");
    assert_eq!(v["head"]["nodes"].as_array().unwrap().len(), 1);
}

#[test]
fn unsupported_video_export_is_rejected_honestly() {
    let f = repo();
    let o = chronicle(&["--export", "x.mp4"], f.path());
    assert!(!o.status.success());
    assert!(stderr(&o).contains("not supported"), "{}", stderr(&o));
    assert!(!f.path().join("x.mp4").exists());
}

#[test]
fn bad_granularity_and_non_repo_fail_cleanly() {
    let f = repo();
    let o = chronicle(&["--export", "-", "-g", "fortnight"], f.path());
    assert!(!o.status.success() && stderr(&o).contains("fortnight"));
    let d = tempfile::tempdir().unwrap();
    let o = chronicle(&["--export", "-"], d.path());
    assert!(!o.status.success());
    assert!(stderr(&o).contains("not a git repository"), "{}", stderr(&o));
}

#[test]
fn since_window_with_no_commits_is_an_error_not_a_panic() {
    let f = repo();
    let o = chronicle(&["--export", "-", "--since", "2099-01-01"], f.path());
    assert!(!o.status.success());
    assert!(stderr(&o).contains("window"), "{}", stderr(&o));
}
