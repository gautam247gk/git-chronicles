//! Thin wrapper over the system `git` binary. History is read as one
//! `git log --raw --numstat` stream; blobs through a persistent `cat-file --batch`.
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

#[derive(Debug, Clone)]
pub struct Repo {
    dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Change {
    pub path: String,
    /// New blob SHA; `None` when the file was deleted.
    pub blob: Option<String>,
    pub added: u64,
    pub removed: u64,
}

#[derive(Debug, Clone)]
pub struct HistCommit {
    pub sha: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub subject: String,
    pub changes: Vec<Change>,
}

pub struct BlobReader {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Drop for BlobReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl BlobReader {
    /// Blob bytes, or `None` if the object does not exist.
    pub fn read(&mut self, sha: &str) -> Result<Option<Vec<u8>>> {
        writeln!(self.stdin, "{sha}")?;
        self.stdin.flush()?;
        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        let parts: Vec<&str> = header.split_whitespace().collect();
        match parts.as_slice() {
            [_, "missing"] => Ok(None),
            [_, _, size] => {
                let mut buf = vec![0u8; size.parse()?];
                self.stdout.read_exact(&mut buf)?;
                let mut nl = [0u8; 1];
                self.stdout.read_exact(&mut nl)?;
                Ok(Some(buf))
            }
            _ => bail!("unexpected cat-file output: {header:?}"),
        }
    }
}

impl Repo {
    pub fn open(path: &Path) -> Result<Repo> {
        let repo = Repo {
            dir: path.to_path_buf(),
        };
        repo.run(&["rev-parse", "--git-dir"])
            .with_context(|| format!("{} is not a git repository", path.display()))?;
        if repo.run(&["rev-list", "-n", "1", "--all"])?.trim().is_empty() {
            bail!("repository {} has no commits", path.display());
        }
        Ok(repo)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new("git");
        c.arg("-C").arg(&self.dir).args(["-c", "core.quotepath=off"]);
        c
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let out = self
            .cmd()
            .args(args)
            .output()
            .context("failed to run git (is it installed?)")?;
        if !out.status.success() {
            bail!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// First-parent history of `rev`, oldest first. Merge commits are diffed
    /// against their first parent, so mainline tree states are exact.
    pub fn history(&self, rev: &str) -> Result<Vec<HistCommit>> {
        let fmt = "--format=%x1e%H%x1f%an%x1f%ae%x1f%at%x1f%s";
        let out = self.run(&[
            "log",
            "--first-parent",
            "-m",
            "--reverse",
            "--no-renames",
            "--no-abbrev",
            "--no-color",
            "--raw",
            "--numstat",
            fmt,
            rev,
            "--",
        ])?;
        let mut commits: Vec<HistCommit> = Vec::new();
        let mut stats: HashMap<String, (u64, u64)> = HashMap::new();
        for line in out.lines() {
            if let Some(h) = line.strip_prefix('\u{1e}') {
                flush_stats(commits.last_mut(), &mut stats);
                let f: Vec<&str> = h.splitn(5, '\u{1f}').collect();
                if f.len() < 5 {
                    bail!("malformed git log header: {h:?}");
                }
                commits.push(HistCommit {
                    sha: f[0].into(),
                    author: f[1].into(),
                    email: f[2].into(),
                    time: f[3].parse().unwrap_or(0),
                    subject: f[4].into(),
                    changes: Vec::new(),
                });
            } else if let (Some(rest), Some(c)) = (line.strip_prefix(':'), commits.last_mut()) {
                let Some((meta, path)) = rest.split_once('\t') else {
                    continue;
                };
                let m: Vec<&str> = meta.split_whitespace().collect();
                if m.len() < 5 || m[0] == "160000" || m[1] == "160000" || path.starts_with('"') {
                    continue; // submodule, or a path git had to quote (unsupported)
                }
                let blob = (!m[4].starts_with('D')).then(|| m[3].to_string());
                c.changes.push(Change {
                    path: path.into(),
                    blob,
                    added: 0,
                    removed: 0,
                });
            } else if let Some(n) = parse_numstat(line) {
                stats.insert(n.0, (n.1, n.2));
            }
        }
        flush_stats(commits.last_mut(), &mut stats);
        Ok(commits)
    }

    pub fn blob_reader(&self) -> Result<BlobReader> {
        let mut child = self
            .cmd()
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to spawn git cat-file")?;
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Ok(BlobReader { child, stdin, stdout })
    }

    /// path -> blob SHA for every regular file at `rev`.
    pub fn snapshot(&self, rev: &str) -> Result<BTreeMap<String, String>> {
        let out = self.run(&["ls-tree", "-r", "-z", "--full-tree", rev])?;
        let mut m = BTreeMap::new();
        for e in out.split('\0').filter(|e| !e.is_empty()) {
            let Some((meta, path)) = e.split_once('\t') else {
                continue;
            };
            let f: Vec<&str> = meta.split(' ').collect();
            if f.len() == 3 && f[1] == "blob" {
                m.insert(path.to_string(), f[2].to_string());
            }
        }
        Ok(m)
    }

    /// First-parent commits of `rev` inside the git-parsed date window.
    pub fn window(&self, rev: &str, since: Option<&str>, until: Option<&str>) -> Result<HashSet<String>> {
        let mut args = vec!["rev-list".to_string(), "--first-parent".into()];
        if let Some(s) = since {
            args.push(format!("--since={s}"));
        }
        if let Some(u) = until {
            args.push(format!("--until={u}"));
        }
        args.push(rev.into());
        args.push("--".into());
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        Ok(self.run(&a)?.lines().map(str::to_string).collect())
    }

    /// commit SHA -> tag names (annotated tags are peeled).
    pub fn tags(&self) -> Result<HashMap<String, Vec<String>>> {
        let out = self.run(&[
            "for-each-ref",
            "refs/tags",
            "--format=%(refname:short)\t%(objectname)\t%(*objectname)",
        ])?;
        let mut m: HashMap<String, Vec<String>> = HashMap::new();
        for l in out.lines() {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() == 3 {
                let commit = if f[2].is_empty() { f[1] } else { f[2] };
                m.entry(commit.to_string()).or_default().push(f[0].to_string());
            }
        }
        Ok(m)
    }

    pub fn changed_paths(&self, a: &str, b: &str) -> Result<Vec<String>> {
        let out = self.run(&["diff", "--name-only", "-z", a, b, "--"])?;
        Ok(out
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect())
    }

    pub fn resolve(&self, rev: &str) -> Result<String> {
        Ok(self
            .run(&["rev-parse", "--verify", &format!("{rev}^{{commit}}")])?
            .trim()
            .to_string())
    }
}

fn parse_numstat(line: &str) -> Option<(String, u64, u64)> {
    let mut it = line.splitn(3, '\t');
    let (a, r, p) = (it.next()?, it.next()?, it.next()?);
    let num = |s: &str| if s == "-" { Some(0) } else { s.parse().ok() };
    Some((p.to_string(), num(a)?, num(r)?))
}

fn flush_stats(c: Option<&mut HistCommit>, stats: &mut HashMap<String, (u64, u64)>) {
    if let Some(c) = c {
        for ch in &mut c.changes {
            if let Some((a, r)) = stats.get(&ch.path) {
                (ch.added, ch.removed) = (*a, *r);
            }
        }
    }
    stats.clear();
}

/// Clone a remote URL into `dest` as a bare repo (full history; no blob filtering).
pub fn clone_bare(url: &str, dest: &Path) -> Result<Repo> {
    let st = Command::new("git")
        .args(["clone", "--bare", "--quiet", url])
        .arg(dest)
        .status()?;
    if !st.success() {
        bail!("git clone {url} failed");
    }
    Repo::open(dest)
}

pub fn looks_remote(s: &str) -> bool {
    s.contains("://") || s.starts_with("git@")
}
