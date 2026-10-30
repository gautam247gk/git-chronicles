//! Audit, blame-edge and PR-diff reports built on top of the analysis.
use crate::analysis::{Analysis, Cycle, Granularity, Options, analyze, graph_at};
use crate::config::Config;
use crate::git::Repo;
use anyhow::{Result, bail};
use globset::Glob;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub from: String,
    pub to: String,
    pub rule: String,
}

#[derive(Debug, Serialize)]
pub struct Risk {
    pub file: String,
    pub top_author: String,
    pub author_share: f64,
    pub fan_in: u32,
    pub owner_inactive: bool,
}

#[derive(Debug, Serialize)]
pub struct Hot {
    pub file: String,
    pub score: f64,
    pub churn: u32,
    pub fan_in: u32,
}

#[derive(Debug, Serialize)]
pub struct Audit {
    pub head: String,
    pub files: usize,
    pub edges: usize,
    pub cycles: Vec<Cycle>,
    pub forbidden: Vec<Violation>,
    pub bus_factor_risks: Vec<Risk>,
    pub orphans: Vec<String>,
    pub hotspots: Vec<Hot>,
}

pub fn find_forbidden(edges: &[[String; 2]], cfg: &Config) -> Result<Vec<Violation>> {
    let mut out = Vec::new();
    for r in &cfg.architecture.boundaries.forbidden_edges {
        let (from, to) = (
            Glob::new(&r.from)?.compile_matcher(),
            Glob::new(&r.to)?.compile_matcher(),
        );
        for [a, b] in edges {
            if from.is_match(a) && to.is_match(b) {
                out.push(Violation {
                    from: a.clone(),
                    to: b.clone(),
                    rule: format!("{} -> {}", r.from, r.to),
                });
            }
        }
    }
    Ok(out)
}

pub fn build_audit(a: Analysis, cfg: &Config) -> Result<Audit> {
    let forbidden = find_forbidden(&a.head.edges, cfg)?;
    let n = &a.head.nodes;
    let bus_factor_risks = n
        .iter()
        .filter(|x| x.silo)
        .map(|x| Risk {
            file: x.id.clone(),
            top_author: x.top_author.clone(),
            author_share: x.author_share,
            fan_in: x.fan_in,
            owner_inactive: x.owner_inactive,
        })
        .collect();
    let orphans = n.iter().filter(|x| x.orphan).map(|x| x.id.clone()).collect();
    let mut hot: Vec<Hot> = n
        .iter()
        .filter(|x| x.hotspot > 0.0)
        .map(|x| Hot {
            file: x.id.clone(),
            score: x.hotspot,
            churn: x.churn,
            fan_in: x.fan_in,
        })
        .collect();
    hot.sort_by(|x, y| y.score.total_cmp(&x.score).then(x.file.cmp(&y.file)));
    hot.truncate(10);
    Ok(Audit {
        head: a.head.sha.chars().take(7).collect(),
        files: n.len(),
        edges: a.head.edges.len(),
        cycles: a.head.cycles,
        forbidden,
        bus_factor_risks,
        orphans,
        hotspots: hot,
    })
}

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

pub fn render_audit(r: &Audit, fmt: &str) -> Result<String> {
    let md = match fmt {
        "table" => false,
        "markdown" => true,
        "json" => return Ok(serde_json::to_string_pretty(r)?),
        "sarif" => return sarif(r),
        _ => bail!("unknown format {fmt:?} (expected table|json|markdown|sarif)"),
    };
    let mut s = String::new();
    let h = |s: &mut String, t: &str, n: usize| {
        if md {
            let _ = writeln!(s, "\n## {t}\n");
        } else {
            let _ = writeln!(s, "\n{t} ({n})");
        }
    };
    let li = if md { "- " } else { "  " };
    let _ = writeln!(
        s,
        "{}git-chronicle audit @ {}: {} files, {} edges",
        if md { "# " } else { "" },
        r.head,
        r.files,
        r.edges
    );
    h(&mut s, "Circular dependencies", r.cycles.len());
    for c in &r.cycles {
        let files: Vec<String> = c
            .files
            .iter()
            .map(|f| if md { format!("`{f}`") } else { f.clone() })
            .collect();
        let _ = writeln!(s, "{li}{}", files.join(", "));
        let edges: Vec<String> = c
            .closing_edges
            .iter()
            .map(|e| format!("{} -> {}", e[0], e[1]))
            .collect();
        let _ = writeln!(
            s,
            "{li}{li}closed by \"{}\" ({}, {}) via {}",
            c.subject,
            &c.introduced_sha[..7.min(c.introduced_sha.len())],
            c.introduced_date,
            edges.join(", ")
        );
    }
    h(&mut s, "Forbidden dependencies", r.forbidden.len());
    for v in &r.forbidden {
        let _ = writeln!(s, "{li}{} -> {}  (rule: {})", v.from, v.to, v.rule);
    }
    h(&mut s, "Bus-factor risks", r.bus_factor_risks.len());
    for x in &r.bus_factor_risks {
        let gone = if x.owner_inactive { ", owner inactive" } else { "" };
        let _ = writeln!(
            s,
            "{li}{}: {} wrote {}, fan-in {}{gone}",
            x.file,
            x.top_author,
            pct(x.author_share),
            x.fan_in
        );
    }
    h(&mut s, "Orphans", r.orphans.len());
    for o in &r.orphans {
        let _ = writeln!(s, "{li}{o}");
    }
    h(&mut s, "Hotspots", r.hotspots.len());
    for x in &r.hotspots {
        let _ = writeln!(
            s,
            "{li}{}: score {:.2} (churn {}, fan-in {})",
            x.file, x.score, x.churn, x.fan_in
        );
    }
    Ok(s)
}

fn sarif(r: &Audit) -> Result<String> {
    let loc = |f: &str| json!({"physicalLocation": {"artifactLocation": {"uri": f}}});
    let mut results = Vec::new();
    for c in &r.cycles {
        results.push(json!({"ruleId": "chronicle/cycle", "level": "warning",
            "message": {"text": format!("Import cycle between {} files introduced by \"{}\"", c.files.len(), c.subject)},
            "locations": c.files.iter().map(|f| loc(f)).collect::<Vec<_>>()}));
    }
    for v in &r.forbidden {
        results.push(json!({"ruleId": "chronicle/forbidden-edge", "level": "error",
            "message": {"text": format!("{} imports {} (forbidden by rule {})", v.from, v.to, v.rule)},
            "locations": [loc(&v.from)]}));
    }
    for x in &r.bus_factor_risks {
        results.push(json!({"ruleId": "chronicle/bus-factor", "level": "note",
            "message": {"text": format!("{} wrote {} of {} (fan-in {})", x.top_author, pct(x.author_share), x.file, x.fan_in)},
            "locations": [loc(&x.file)]}));
    }
    let rule = |id: &str, t: &str| json!({"id": id, "shortDescription": {"text": t}});
    Ok(serde_json::to_string_pretty(&json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{"tool": {"driver": {"name": "git-chronicle", "version": env!("CARGO_PKG_VERSION"), "rules": [
            rule("chronicle/cycle", "Circular import dependency"),
            rule("chronicle/forbidden-edge", "Import crosses a forbidden architecture boundary"),
            rule("chronicle/bus-factor", "Heavily depended-on file owned by one author")]}},
            "results": results}]
    }))?)
}

/// Text report for `blame-edge`; errors if the edge never existed in the analyzed history.
pub fn blame_edge(a: &Analysis, cfg: &Config, from: &str, to: &str) -> Result<String> {
    let norm = |p: &str| p.trim_start_matches("./").to_string();
    let key = (norm(from), norm(to));
    let Some(&i) = a.edge_first_seen.get(&key) else {
        bail!(
            "no import edge {} -> {} found in the analyzed history",
            key.0,
            key.1
        );
    };
    let c = &a.commits[i];
    let edge = [key.0.clone(), key.1.clone()];
    let mut s = format!(
        "Edge Introduced in Commit: {} ({})\nAuthor: {} <{}>\nCommit Message: \"{}\"\nImpact: Created a direct coupling between {} and {}",
        c.short, c.date, c.author, c.email, c.subject, key.0, key.1
    );
    let viol = find_forbidden(std::slice::from_ref(&edge), cfg)?;
    if let Some(v) = viol.first() {
        let _ = write!(s, " (+1 boundary violation: rule {})", v.rule);
    }
    s.push('\n');
    if let Some(cy) = a.graph_deltas[i]
        .cycles_introduced
        .iter()
        .find(|cy| cy.contains(&edge[0]) && cy.contains(&edge[1]))
    {
        let _ = writeln!(s, "This edge closed an import cycle: {}", cy.join(", "));
    }
    if !a.head.edges.contains(&edge) {
        s.push_str("Note: this edge is no longer present at HEAD.\n");
    }
    Ok(s)
}

#[derive(Debug, Serialize)]
pub struct PrDiff {
    pub base: String,
    pub head: String,
    pub compared_from: String,
    pub files_added: Vec<String>,
    pub files_removed: Vec<String>,
    pub edges_added: Vec<[String; 2]>,
    pub edges_removed: Vec<[String; 2]>,
    pub new_cycles: Vec<Vec<String>>,
    pub new_forbidden: Vec<Violation>,
    pub warnings: Vec<String>,
}

impl PrDiff {
    pub fn is_empty(&self) -> bool {
        self.files_added.is_empty()
            && self.files_removed.is_empty()
            && self.edges_added.is_empty()
            && self.edges_removed.is_empty()
            && self.new_cycles.is_empty()
    }
}

/// Compare the import graph at the merge-base of `base`/`head` (or `base` if unrelated) with `head`.
pub fn pr_diff(repo: &Repo, cfg: &Config, opts: &Options, base: &str, head: &str) -> Result<PrDiff> {
    let (base_sha, head_sha) = (repo.resolve(base)?, repo.resolve(head)?);
    let from = repo
        .merge_base(&base_sha, &head_sha)
        .unwrap_or_else(|| base_sha.clone());
    let (g0, g1) = (
        graph_at(repo, cfg, opts, &from)?,
        graph_at(repo, cfg, opts, &head_sha)?,
    );
    let pairs = |s: &BTreeSet<(String, String)>| -> Vec<[String; 2]> {
        s.iter().map(|e| [e.0.clone(), e.1.clone()]).collect()
    };
    let added: BTreeSet<_> = g1.edges.difference(&g0.edges).cloned().collect();
    let removed: BTreeSet<_> = g0.edges.difference(&g1.edges).cloned().collect();
    let old_cycles: BTreeSet<&Vec<String>> = g0.cycles.iter().collect();
    let new_cycles = g1
        .cycles
        .iter()
        .filter(|c| !old_cycles.contains(c))
        .cloned()
        .collect();
    let edges_added = pairs(&added);
    let new_forbidden = find_forbidden(&edges_added, cfg)?;

    let mut warnings = Vec::new();
    let changed: BTreeSet<String> = repo.changed_paths(&from, &head_sha)?.into_iter().collect();
    let o = Options {
        rev: head_sha.clone(),
        granularity: Some(Granularity::Month),
        since: None,
        until: None,
        ..opts.clone()
    };
    for n in analyze(repo, cfg, &o)?
        .head
        .nodes
        .iter()
        .filter(|n| changed.contains(&n.id))
    {
        if n.silo {
            warnings.push(format!(
                "`{}` is a single-owner silo ({} wrote {}, fan-in {})",
                n.id,
                n.top_author,
                pct(n.author_share),
                n.fan_in
            ));
        } else if n.churn >= 5 && n.author_share > 0.8 {
            warnings.push(format!(
                "`{}` is a high-churn single-owner file ({} changes in window, {} wrote {})",
                n.id,
                n.churn,
                n.top_author,
                pct(n.author_share)
            ));
        }
    }
    Ok(PrDiff {
        base: base.into(),
        head: head.into(),
        compared_from: from.chars().take(7).collect(),
        files_added: g1.files.difference(&g0.files).cloned().collect(),
        files_removed: g0.files.difference(&g1.files).cloned().collect(),
        edges_added,
        edges_removed: pairs(&removed),
        new_cycles,
        new_forbidden,
        warnings,
    })
}

pub fn render_pr_diff(d: &PrDiff, fmt: &str) -> Result<String> {
    match fmt {
        "json" => return Ok(serde_json::to_string_pretty(d)?),
        "markdown" => {}
        _ => bail!("unknown format {fmt:?} (expected markdown|json)"),
    }
    let mut s = format!(
        "## Architecture diff: `{}` ← `{}`\n\n(compared from merge-base `{}`)\n\n",
        d.base, d.head, d.compared_from
    );
    if d.is_empty() {
        s.push_str("No architectural changes.\n");
        return Ok(s);
    }
    let _ = writeln!(s, "| Change | Count |\n| --- | --- |");
    let _ = writeln!(
        s,
        "| New modules | {} |\n| Deleted modules | {} |",
        d.files_added.len(),
        d.files_removed.len()
    );
    let _ = writeln!(
        s,
        "| Dependency edges | +{} edges, -{} edges |",
        d.edges_added.len(),
        d.edges_removed.len()
    );
    let _ = writeln!(s, "| New circular dependencies | {} |\n", d.new_cycles.len());
    let list = |s: &mut String, title: &str, items: Vec<String>| {
        if !items.is_empty() {
            let _ = writeln!(s, "### {title}\n");
            for i in items {
                let _ = writeln!(s, "- {i}");
            }
            s.push('\n');
        }
    };
    list(
        &mut s,
        "New modules",
        d.files_added.iter().map(|f| format!("`{f}`")).collect(),
    );
    list(
        &mut s,
        "Deleted modules",
        d.files_removed.iter().map(|f| format!("`{f}`")).collect(),
    );
    list(
        &mut s,
        "New dependencies",
        d.edges_added
            .iter()
            .map(|e| format!("`{}` → `{}`", e[0], e[1]))
            .collect(),
    );
    list(
        &mut s,
        "Removed dependencies",
        d.edges_removed
            .iter()
            .map(|e| format!("`{}` → `{}`", e[0], e[1]))
            .collect(),
    );
    list(
        &mut s,
        "New circular dependencies",
        d.new_cycles
            .iter()
            .map(|c| c.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(" ↔ "))
            .collect(),
    );
    list(
        &mut s,
        "Forbidden boundary violations",
        d.new_forbidden
            .iter()
            .map(|v| format!("`{}` → `{}` (rule {})", v.from, v.to, v.rule))
            .collect(),
    );
    list(&mut s, "Risk warnings", d.warnings.clone());
    Ok(s)
}
