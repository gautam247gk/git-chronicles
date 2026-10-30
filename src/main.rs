use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use git_chronicle::analysis::{Granularity, Options, analyze};
use git_chronicle::cli::{Common, Filters, open};
use git_chronicle::config::Config;
use git_chronicle::report::{self, build_audit, render_audit, render_pr_diff};
use git_chronicle::viewer::{open_browser, render_html, serve};

#[derive(Parser)]
#[command(
    name = "git-chronicle",
    version,
    about = "Analyze Git history and import graphs to see architecture evolve",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    #[command(flatten)]
    view: ViewArgs,
}

#[derive(Args)]
struct ViewArgs {
    #[command(flatten)]
    common: Common,
    /// Write the timeline to FILE instead of serving it (.json, .html, or "-" for JSON on stdout).
    #[arg(long, value_name = "FILE")]
    export: Option<String>,
    /// Viewer colour theme: github-dark, cyberpunk, catppuccin, dracula, minimal-light.
    #[arg(long)]
    theme: Option<String>,
    /// Initial overlay: architecture, hotspots, bus-factor, cycles, orphans.
    #[arg(short, long, default_value = "architecture")]
    overlay: String,
    /// Port for the local viewer server.
    #[arg(short, long, default_value_t = 4400)]
    port: u16,
    /// Do not open the system browser.
    #[arg(long)]
    no_open: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check HEAD for import cycles, forbidden edges and bus-factor risk (non-zero exit for CI).
    Audit {
        #[command(flatten)]
        common: Common,
        /// Output format: table, json, markdown or sarif.
        #[arg(long, default_value = "table")]
        format: String,
        /// Exit 1 if any import cycle exists at HEAD.
        #[arg(long)]
        fail_on_cycles: bool,
        /// Exit 1 if any `forbidden_edges` rule from .chroniclerc.toml is violated.
        #[arg(long)]
        fail_on_forbidden: bool,
        /// Exit 1 if more than N files are bus-factor risks.
        #[arg(long, value_name = "N")]
        max_bus_factor_risk: Option<usize>,
    },
    /// Compare the dependency graph of two refs (PR architecture diff).
    PrDiff {
        base: String,
        head: String,
        #[arg(default_value = ".")]
        repo: String,
        /// Output format: markdown or json.
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Exit 1 if the branch introduces a new import cycle.
        #[arg(long)]
        fail_on_cycles: bool,
        /// Exit 1 if the branch introduces a forbidden edge.
        #[arg(long)]
        fail_on_forbidden: bool,
        #[command(flatten)]
        filters: Filters,
    },
    /// Find the commit where SOURCE first imported TARGET.
    BlameEdge {
        source: String,
        target: String,
        #[arg(default_value = ".")]
        repo: String,
        /// Branch or revision to search.
        #[arg(short, long, default_value = "HEAD")]
        branch: String,
        #[command(flatten)]
        filters: Filters,
    },
}

fn main() {
    match run(Cli::parse()) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(2);
        }
    }
}

/// Returns the process exit code (1 = a requested check failed).
fn run(cli: Cli) -> Result<i32> {
    match cli.cmd {
        None => default_cmd(cli.view).map(|()| 0),
        Some(Cmd::Audit {
            common,
            format,
            fail_on_cycles,
            fail_on_forbidden,
            max_bus_factor_risk,
        }) => {
            let (a, cfg) = common.run()?;
            let r = build_audit(a, &cfg)?;
            print!("{}", render_audit(&r, &format)?);
            let failed = (fail_on_cycles && !r.cycles.is_empty())
                || (fail_on_forbidden && !r.forbidden.is_empty())
                || max_bus_factor_risk.is_some_and(|n| r.bus_factor_risks.len() > n);
            Ok(i32::from(failed))
        }
        Some(Cmd::PrDiff {
            base,
            head,
            repo,
            format,
            fail_on_cycles,
            fail_on_forbidden,
            filters,
        }) => {
            let opened = open(&repo)?;
            let cfg = Config::load(opened.repo.dir())?;
            let opts = Options {
                paths: filters.paths,
                exclude: filters.exclude,
                ..Options::default()
            };
            let d = report::pr_diff(&opened.repo, &cfg, &opts, &base, &head)?;
            print!("{}", render_pr_diff(&d, &format)?);
            let failed = (fail_on_cycles && !d.new_cycles.is_empty())
                || (fail_on_forbidden && !d.new_forbidden.is_empty());
            Ok(i32::from(failed))
        }
        Some(Cmd::BlameEdge {
            source,
            target,
            repo,
            branch,
            filters,
        }) => {
            let opened = open(&repo)?;
            let cfg = Config::load(opened.repo.dir())?;
            // exact commit attribution needs every commit as its own sample
            let opts = Options {
                rev: branch,
                granularity: Some(Granularity::Commit),
                paths: filters.paths,
                exclude: filters.exclude,
                ..Options::default()
            };
            let a = analyze(&opened.repo, &cfg, &opts)?;
            print!("{}", report::blame_edge(&a, &cfg, &source, &target)?);
            Ok(0)
        }
    }
}

fn default_cmd(v: ViewArgs) -> Result<()> {
    let ext = v
        .export
        .as_deref()
        .and_then(|o| o.rsplit_once('.'))
        .map(|x| x.1.to_ascii_lowercase());
    match (v.export.as_deref(), ext.as_deref()) {
        (None | Some("-"), _) | (_, Some("json" | "html")) => {}
        (_, Some("mp4" | "webm" | "gif")) => bail!("video/GIF export is not supported; use .html or .json"),
        (Some(o), _) => bail!("unsupported export format for {o:?}; use .html or .json"),
    }
    let (analysis, cfg) = v.common.run()?;
    let theme = v.theme.unwrap_or(cfg.export.theme);
    match v.export.as_deref() {
        Some("-") => println!("{}", serde_json::to_string(&analysis)?),
        Some(o) if ext.as_deref() == Some("json") => std::fs::write(o, serde_json::to_string(&analysis)?)?,
        Some(o) => std::fs::write(o, render_html(&analysis, &theme, &v.overlay)?)?,
        None => {
            let html = render_html(&analysis, &theme, &v.overlay)?;
            serve(&html, v.port, |url| {
                println!("git-chronicle viewer: {url} (Ctrl-C to stop)");
                if !v.no_open {
                    open_browser(url);
                }
            })?;
        }
    }
    Ok(())
}
