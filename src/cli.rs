//! Argument groups and repo opening shared by every command.
use crate::analysis::{Analysis, Granularity, Options, analyze};
use crate::config::Config;
use crate::git::{self, Repo};
use anyhow::{Context, Result};
use clap::Args;
use std::path::{Path, PathBuf};

#[derive(Args, Debug, Clone)]
pub struct Common {
    /// Local Git repository path or remote URL (cloned in full into a temp dir).
    #[arg(default_value = ".")]
    pub repo: String,
    /// Branch or revision to walk (first-parent history).
    #[arg(short, long, default_value = "HEAD")]
    pub branch: String,
    /// Start of the analysis window (any date git understands, e.g. "6 months ago").
    #[arg(long)]
    pub since: Option<String>,
    /// End of the analysis window.
    #[arg(long)]
    pub until: Option<String>,
    /// Sampling: auto, commit, day, week, month or tag [default: config file, else auto].
    #[arg(short, long)]
    pub granularity: Option<String>,
    /// Only analyze paths matching this glob (repeatable).
    #[arg(long = "path")]
    pub paths: Vec<String>,
    /// Exclude paths matching this glob (repeatable), on top of default ignores.
    #[arg(short, long)]
    pub exclude: Vec<String>,
}

impl Common {
    pub fn options(&self) -> Result<Options> {
        Ok(Options {
            rev: self.branch.clone(),
            since: self.since.clone(),
            until: self.until.clone(),
            granularity: self.granularity.as_deref().map(Granularity::parse).transpose()?,
            paths: self.paths.clone(),
            exclude: self.exclude.clone(),
        })
    }

    pub fn run(&self) -> Result<(Analysis, Config)> {
        let opened = open(&self.repo)?;
        let cfg = Config::load(opened.repo.dir())?;
        let a = analyze(&opened.repo, &cfg, &self.options()?)?;
        Ok((a, cfg))
    }
}

/// An opened repo; a temp clone is removed on drop.
pub struct Opened {
    pub repo: Repo,
    temp: Option<PathBuf>,
}

impl Drop for Opened {
    fn drop(&mut self) {
        if let Some(t) = &self.temp {
            let _ = std::fs::remove_dir_all(t);
        }
    }
}

pub fn open(spec: &str) -> Result<Opened> {
    if git::looks_remote(spec) {
        let dest = std::env::temp_dir().join(format!("git-chronicle-{}", std::process::id()));
        let repo = git::clone_bare(spec, &dest).with_context(|| format!("cloning {spec}"))?;
        return Ok(Opened {
            repo,
            temp: Some(dest),
        });
    }
    Ok(Opened {
        repo: Repo::open(Path::new(spec))?,
        temp: None,
    })
}
