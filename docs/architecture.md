# Architecture

```text
git (system binary)
  ├─ log --first-parent -m --raw --numstat   → HistCommit[] (path, new blob SHA, +/- lines)   src/git.rs
  ├─ cat-file --batch (one persistent proc)  → blob bytes
  ├─ ls-tree / merge-base / diff / for-each-ref (snapshots, PR diff, tags)
  ▼
analysis::analyze   src/analysis.rs
  state: path → Parsed{loc, imports}     parse cache keyed by blob SHA (unchanged blobs never re-read)
  at each sample: resolve imports against the file set → edge set → delta vs previous sample
  ▼
graph::sccs (iterative Tarjan)   src/graph.rs   → cycles; introduced-in tracking
lang::{extract,resolve}          src/lang.rs    → TS/JS, Python, Rust regex extraction + resolution
  ▼
Analysis { commits[], graph_deltas[], head{nodes,edges,cycles} }  ── serde JSON
  ├─ viewer.rs + assets/viewer.html : JSON inlined into a single canvas page (served or exported)
  └─ report.rs : audit / blame-edge / pr-diff renderers
```

Modules: `cli.rs` shared clap argument groups and repo opening; `config.rs` `.chroniclerc.toml`; `dates.rs` UTC calendar math (no date crate).

## Why shell out to `git`

It avoids a libgit2 build dependency and works wherever git does. History is one streamed `git log`; blobs come through one `cat-file --batch` process, so there is no per-file process spawn. The cost is that `git` must be installed.

## Timeline JSON

```jsonc
{
  "version": 1, "project": null, "theme": "github-dark", "rev": "HEAD", "granularity": "commit",
  "commits": [ { "sha", "short", "author", "time", "date", "subject", "commit_count",
                 "added", "removed", "files", "loc", "edges", "cycles", "tags" } ],
  "graph_deltas": [ { "files_added", "files_removed", "edges_added": [["a","b"]],
                      "edges_removed", "cycles_introduced": [["a","b","c"]] } ],   // parallel to commits
  "head": { "sha", "nodes": [ { "id", "loc", "module", "fan_in", "fan_out", "churn", "top_author",
                                "author_share", "owner_inactive", "hotspot", "silo", "orphan" } ],
            "edges": [["importer","imported"]],
            "cycles": [ { "files", "introduced_sha", "introduced_date", "subject", "closing_edges" } ] }
}
```

`commits[i]` and `graph_deltas[i]` describe the same sample; the first delta is relative to the empty graph. Author e-mail addresses are not exported.

## Metric definitions

- **LOC**: non-blank lines of the file at that commit.
- **Fan-in / fan-out**: distinct files importing / imported by the file (self-imports ignored).
- **Churn**: number of analyzed commits that touched the file within `hotspot_window_days` before the last analyzed commit.
- **Hotspot** = `churn × log10(max(LOC,1)) × fan_in` (spec formula; zero for files nobody imports).
- **Author share**: lines *added* by the top author ÷ lines added by everyone, over the file's whole analyzed history. This approximates `git blame` ownership; it does not account for lines that were later rewritten or deleted.
- **Silo / bus-factor risk**: author share > 0.8 **and** fan-in ≥ 5. `owner_inactive` additionally flags a top author with no commit in `inactive_author_days`.
- **Cycle**: strongly connected component with more than one file. The "culprit" is the first sample where that exact member set appeared; `closing_edges` are the edges added in that sample inside the component. If a cycle later grows, the larger set is reported as a new cycle at that commit.
- **Orphan**: fan-in 0, not a configured or conventional entrypoint.

## Viewer

One HTML file, no network access, no external scripts. Scrubbing replays deltas; cycles in the viewer are recomputed in the browser per sample. Node layout is a small grid-accelerated force simulation (fine for a few thousand files). Overlays other than *architecture* and *cycles* use the HEAD metrics above, because the JSON only carries metrics at HEAD.

The server is a minimal single-threaded `std::net` HTTP responder bound to `127.0.0.1`; it serves the page for `GET /` and 404s everything else.
