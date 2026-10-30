use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use git_chronicle::cli::Common;

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

#[derive(clap::Args)]
struct ViewArgs {
    #[command(flatten)]
    common: Common,
    /// Write the timeline to FILE (.json, or "-" for stdout).
    #[arg(long, value_name = "FILE")]
    export: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(2);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Some(c) => match c {},
        None => default_cmd(cli.view),
    }
}

fn default_cmd(v: ViewArgs) -> Result<()> {
    let Some(out) = &v.export else {
        bail!("no --export given; the interactive viewer is not built yet")
    };
    let ext = out.rsplit_once('.').map(|x| x.1.to_ascii_lowercase());
    match (out.as_str(), ext.as_deref()) {
        ("-", _) | (_, Some("json")) => {}
        (_, Some("mp4" | "webm" | "gif")) => bail!("video/GIF export is not supported; use .json"),
        _ => bail!("unsupported export format for {out:?}; use .json"),
    }
    let (analysis, _) = v.common.run()?;
    let json = serde_json::to_string(&analysis)?;
    if out == "-" {
        println!("{json}")
    } else {
        std::fs::write(out, json)?
    }
    Ok(())
}
