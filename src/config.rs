//! `.chroniclerc.toml` loading. Unknown keys are ignored so the spec's
//! export keys (fps, resolution, ...) parse even though video export is not implemented.
use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use std::path::Path;

pub const FILE_NAME: &str = ".chroniclerc.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub project: Project,
    pub analysis: Analysis,
    pub architecture: Architecture,
    pub export: Export,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Project {
    pub name: Option<String>,
    pub entrypoints: Vec<String>,
    pub ignore: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Analysis {
    pub granularity: String,
    pub hotspot_window_days: u32,
    pub inactive_author_days: u32,
}

impl Default for Analysis {
    fn default() -> Self {
        Analysis {
            granularity: "auto".into(),
            hotspot_window_days: 90,
            inactive_author_days: 180,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Architecture {
    pub boundaries: Boundaries,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Boundaries {
    pub forbidden_edges: Vec<ForbiddenEdge>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForbiddenEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Export {
    pub theme: String,
}

impl Default for Export {
    fn default() -> Self {
        Export {
            theme: "github-dark".into(),
        }
    }
}

impl Config {
    /// Read `<dir>/.chroniclerc.toml`; defaults if it does not exist.
    pub fn load(dir: &Path) -> Result<Config> {
        let p = dir.join(FILE_NAME);
        match std::fs::read_to_string(&p) {
            Ok(s) => Config::parse(&s).with_context(|| format!("invalid {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", p.display())),
        }
    }

    pub fn parse(s: &str) -> Result<Config> {
        Ok(toml::from_str(s)?)
    }
}

pub fn globset(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).with_context(|| format!("bad glob {p:?}"))?);
    }
    Ok(b.build()?)
}
