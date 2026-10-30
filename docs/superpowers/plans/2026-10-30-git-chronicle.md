# git-chronicle Implementation Plan

> **For agentic workers:** use superpowers:executing-plans. Steps are TDD: failing test, watch it fail, minimal code, watch it pass, commit.

**Goal:** A working Rust CLI that analyzes real Git history and import dependencies and reports architecture evolution, cycles, hotspots, bus-factor risk and orphans.

**Architecture:** Shell out to the system `git` binary (one persistent `git cat-file --batch` for blobs, one `git log --raw --numstat` stream for history). Regex-based import extraction per language, cached by blob SHA so unchanged files are never re-parsed. Edges are resolved against the file set of each sampled snapshot. An in-tree iterative Tarjan finds cycles. A self-contained HTML/canvas viewer embeds the timeline JSON.

**Tech Stack:** Rust stable, clap, serde/serde_json, toml, regex, globset, anyhow; tempfile (dev) for fixture repos.

**Spec:** `docs/original-spec.md` (kept unchanged).

## Global Constraints

- Binary name `git-chronicle`; license `MIT OR Apache-2.0`; config file `.chroniclerc.toml`.
- Granularity: `auto|commit|day|week|month|tag`. Overlays: `architecture|hotspots|bus-factor|cycles|orphans`.
- Hotspot = churn_window x log10(LOC) x fan_in. Silo = top author share > 0.8 AND fan_in >= 5. Cycle = SCC with more than one node. Orphan = fan_in 0 and not an entrypoint.
- Supported languages in the MVP: TypeScript/JavaScript, Python, Rust. Nothing else is claimed.

## Honest scope (deliberately NOT built; documented as limitations)

WebGL/PixiJS, `--tui`, mp4/gif export, redb/SQLite cache, tree-sitter, tsconfig aliases, Cargo workspace crate links, Go/C/C++/Java/C#, PR-comment posting, SVG/GIF PR diff, `npx`/brew/winget packaging, rayon parallelism. Substitutes: canvas viewer served by a std-only HTTP server or exported as a standalone `.html`; JSON export; markdown PR diff.

## Review Focus

- Empty repo / no commits: clear error, no panic.
- Repo with files of unsupported languages only: empty graph, exit 0.
- Import cycles of length 1 (self import) and 2: self import is ignored; 2-cycle is reported.
- Deleted-then-re-added files and edges: blame-edge reports the first appearance.
- Paths with spaces and unicode; binary or non-UTF-8 blobs skipped without error.
- Merge commits: first-parent history only, documented.

## Milestones (each ends with fmt, clippy -D warnings, cargo test, commit)

1. **Scaffold + git layer + config:** `git.rs` (history stream, blob reader, tags, ls-tree snapshot, remote clone), `config.rs`. Tests build temp repos with real `git`.
2. **Extractors + resolvers:** `lang.rs` `extract(path, text) -> Vec<Import>`, `resolve(...)` for TS/JS, Python, Rust. Table-driven unit tests.
3. **Engine + `analyze`:** `graph.rs` (edges, SCC, fan-in), `analysis.rs` (walk, sampling, deltas, metrics), timeline JSON via `--export x.json` or stdout. Integration test on a fixture with a cycle introduced in a known commit.
4. **Commands:** `audit` (table/json/markdown/sarif, `--fail-on-cycles`, `--fail-on-forbidden`, `--max-bus-factor-risk`), `blame-edge`, `pr-diff`.
5. **Viewer:** embedded single-file canvas app (scrubber, play, 5 overlays, inspector, keys), `view` default command, `--export x.html`.
6. **Docs and CI:** README, docs/ (usage, config, architecture, limitations), license files, GitHub Actions (fmt, clippy, test).

## Deviations from this plan (recorded after implementation)

- `petgraph` was dropped: its `tarjan_scc` is recursive and overflowed the stack on a 50k-node import chain (caught by `tests/graph.rs`), so `src/graph.rs` has an iterative Tarjan.
- `report.rs` (audit, blame-edge, pr-diff rendering), `cli.rs` and `dates.rs` were added as separate modules.
- The viewer JS is smoke-tested under Node with a stub DOM (`tests/viewer_smoke.js`); no real browser was available on the build host.
- Running against `expressjs/express` found a root-level `require('../')` resolution bug and false test-file orphans; both fixed with tests.
