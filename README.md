# git-chronicle

Analyze a Git repository's **real history and import graph** to see how its architecture evolved: which files depend on which, when import cycles appeared, which files are hotspots or single-owner silos, and who introduced a given dependency.

It is a local-first Rust CLI. It reads history with your system `git`, extracts imports from TypeScript/JavaScript, Python and Rust sources, and produces:

- an interactive, time-scrubbable viewer (one self-contained HTML page, served on localhost or exported to a file),
- a machine-readable timeline (`--export timeline.json`),
- CI-friendly reports: `audit`, `pr-diff`, `blame-edge`.

The original product specification is kept unchanged in [`docs/original-spec.md`](docs/original-spec.md). **This MVP implements a deliberately smaller subset of it** — see [What is and isn't implemented](#what-is-and-isnt-implemented).

## Install

Requirements: a recent stable Rust (1.88+) and `git` on `PATH`. `node` is optional (only used by one test).

```bash
# from a checkout
cargo install --path .

# or straight from GitHub
cargo install --git https://github.com/gautam247gk/git-chronicles

# no install: build and run in place
cargo run --release -- audit
```

If you don't have Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` installs it into `~/.cargo` without sudo.

The binary is named `git-chronicle`, so `git chronicle ...` also works once it is on `PATH`.

## Quick start

```bash
# Open the interactive viewer for the repo in the current directory
git-chronicle                       # http://127.0.0.1:4400, opens your browser
git-chronicle --no-open -p 5000     # don't open a browser, custom port

# Save a standalone HTML page, or the raw timeline
git-chronicle --export chronicle.html --theme cyberpunk --overlay cycles
git-chronicle --export timeline.json
git-chronicle --export - | jq '.head.cycles'

# Narrow the analysis
git-chronicle ~/src/app --path 'packages/api/**' --since "6 months ago" -g week

# CI: fail on import cycles, forbidden edges, or too many bus-factor risks
git-chronicle audit --fail-on-cycles --fail-on-forbidden --max-bus-factor-risk 3
git-chronicle audit --format sarif > chronicle.sarif

# PR architecture diff as a markdown comment body
git-chronicle pr-diff origin/main HEAD --format markdown > pr-comment.md

# Who introduced this dependency?
git-chronicle blame-edge src/auth/session.ts src/db/raw_client.ts
```

A remote URL works anywhere a path does (`git-chronicle audit https://github.com/expressjs/express.git`); the full history is cloned bare into a temp dir and removed afterwards.

Example `audit` output (from a small demo repo):

```text
git-chronicle audit @ 38e9880: 11 files, 9 edges

Circular dependencies (1)
  src/a.ts, src/b.ts, src/c.ts
    closed by "wire b to c (oops)" (e9da967, 2024-03-01) via src/b.ts -> src/c.ts

Bus-factor risks (1)
  src/db/raw.ts: Alice wrote 100%, fan-in 6
```

Exit codes: `0` success, `1` a requested `--fail-on-*` / `--max-*` check failed, `2` usage or runtime error.

## What is and isn't implemented

| Spec item | Status |
| --- | --- |
| Git history walk, per-commit import graph, only changed blobs re-parsed | **Implemented** (system `git`, cached by blob SHA) |
| TypeScript/JavaScript, Python, Rust imports | **Implemented**, regex-based, see [limitations](docs/limitations.md) |
| Go, C/C++, Java, C#; tsconfig path aliases; Cargo workspace crate links | Not implemented |
| Sampling `commit/day/week/month/tag/auto`, `--since/--until/--path/--exclude/--branch` | **Implemented** |
| Five overlays (architecture, hotspots, bus factor, cycles, orphans) | **Implemented** (churn/bus-factor/orphan metrics are HEAD snapshots; cycles are per-commit) |
| Tarjan cycle detection, introducing-commit for each cycle | **Implemented** |
| Inspector: genesis commit, cycle culprit | **Implemented**; temporal-coupling (co-change) top-5 is **not** |
| `audit`, `pr-diff` (markdown/json), `blame-edge`, `.chroniclerc.toml` | **Implemented** |
| Interactive viewer with scrubber, playback, speeds, shortcuts, search | **Implemented** as a 2D canvas page (no WebGL, no PixiJS, no spring-physics morphing) |
| `--tui` terminal UI | Not implemented |
| `--export` mp4/webm/gif, presets, camera director, HUD | Not implemented (rejected with a clear error). `.html` and `.json` only |
| SVG/GIF PR diff, posting PR comments | Not implemented (markdown/JSON only; post it with your CI) |
| `.git/chronicle-cache.db` persistent cache, `--no-cache`, `--threads` | Not implemented (single-threaded, in-memory) |
| `npx`, Homebrew, Scoop/Winget packaging | Not available. Use `cargo install` |

## Documentation

- [Usage and CLI reference](docs/usage.md) — every command, flag, output format, keyboard shortcut
- [Configuration](docs/configuration.md) — `.chroniclerc.toml`
- [Architecture](docs/architecture.md) — pipeline, metrics definitions, data schema
- [Limitations](docs/limitations.md) — what to know before trusting the numbers
- [Implementation plan](docs/superpowers/plans/2026-10-30-git-chronicle.md)

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test            # builds real temp git repos; needs `git`, uses `node` if present
```

CI runs the same on Linux and macOS and also audits this repository for cycles.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
