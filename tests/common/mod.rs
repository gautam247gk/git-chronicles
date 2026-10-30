#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// A throwaway git repository with deterministic authors and dates.
pub struct Fixture {
    pub dir: TempDir,
    clock: i64,
}

impl Fixture {
    pub fn new() -> Self {
        let f = Fixture {
            dir: tempfile::tempdir().unwrap(),
            clock: 1_700_000_000,
        };
        f.git(&["init", "-q", "-b", "main"]);
        f
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn git(&self, args: &[&str]) -> String {
        self.git_as("Alice", args)
    }

    pub fn git_as(&self, author: &str, args: &[&str]) -> String {
        let ts = format!("{} +0000", self.clock);
        let email = format!("{}@example.com", author.to_lowercase());
        let out = Command::new("git")
            .args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", author)
            .env("GIT_AUTHOR_EMAIL", &email)
            .env("GIT_COMMITTER_NAME", author)
            .env("GIT_COMMITTER_EMAIL", &email)
            .env("GIT_AUTHOR_DATE", &ts)
            .env("GIT_COMMITTER_DATE", &ts)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    pub fn write(&self, rel: &str, content: &str) {
        let p: PathBuf = self.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn remove(&self, rel: &str) {
        std::fs::remove_file(self.path().join(rel)).unwrap();
    }

    /// Stage everything and commit one day after the previous commit.
    pub fn commit(&mut self, author: &str, msg: &str) -> String {
        self.clock += 86_400;
        self.git_as(author, &["add", "-A"]);
        self.git_as(author, &["commit", "-q", "-m", msg]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Advance the fake clock by `days` without committing.
    pub fn advance_days(&mut self, days: i64) {
        self.clock += days * 86_400;
    }
}
