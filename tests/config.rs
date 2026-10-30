use git_chronicle::config::{Config, globset};

const SPEC_EXAMPLE: &str = r#"
[project]
name = "Acme Core Engine"
entrypoints = ["src/main.rs", "src/lib.rs"]
ignore = ["**/node_modules/**", "**/*.test.ts"]

[analysis]
granularity = "week"
hotspot_window_days = 30
inactive_author_days = 60

[architecture.boundaries]
forbidden_edges = [
  { from = "src/ui/**", to = "src/database/**" }
]

[export]
theme = "cyberpunk"
"#;

#[test]
fn defaults_when_file_missing() {
    let d = tempfile::tempdir().unwrap();
    let c = Config::load(d.path()).unwrap();
    assert_eq!(c.analysis.hotspot_window_days, 90);
    assert_eq!(c.analysis.inactive_author_days, 180);
    assert_eq!(c.analysis.granularity, "auto");
    assert_eq!(c.export.theme, "github-dark");
    assert!(c.project.entrypoints.is_empty());
}

#[test]
fn parses_spec_example() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join(".chroniclerc.toml"), SPEC_EXAMPLE).unwrap();
    let c = Config::load(d.path()).unwrap();
    assert_eq!(c.project.name.as_deref(), Some("Acme Core Engine"));
    assert_eq!(c.analysis.granularity, "week");
    assert_eq!(c.analysis.hotspot_window_days, 30);
    assert_eq!(c.architecture.boundaries.forbidden_edges[0].from, "src/ui/**");
    assert_eq!(c.export.theme, "cyberpunk");
}

#[test]
fn invalid_toml_names_the_file() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join(".chroniclerc.toml"), "[project\n").unwrap();
    let e = Config::load(d.path()).unwrap_err();
    assert!(format!("{e:#}").contains(".chroniclerc.toml"));
}

#[test]
fn unknown_keys_are_tolerated() {
    let c = Config::parse("[export]\nfps = 60\nresolution = \"1080p\"\n[future]\nx = 1\n").unwrap();
    assert_eq!(c.export.theme, "github-dark");
}

#[test]
fn glob_set_matches_nested_and_root_paths() {
    let g = globset(&["**/node_modules/**".to_string(), "**/*.test.ts".to_string()]).unwrap();
    assert!(g.is_match("node_modules/x/index.js"));
    assert!(g.is_match("web/node_modules/x/index.js"));
    assert!(g.is_match("src/a.test.ts"));
    assert!(!g.is_match("src/a.ts"));
    assert!(globset(&["[".to_string()]).is_err());
}
