# Usage

```text
git-chronicle [OPTIONS] [REPO]            # viewer / export
git-chronicle audit [OPTIONS] [REPO]
git-chronicle pr-diff <BASE> <HEAD> [REPO] [OPTIONS]
git-chronicle blame-edge <SOURCE> <TARGET> [REPO] [OPTIONS]
```

`REPO` is a local path (default `.`) or a remote URL (`https://`, `ssh://`, `git@...`). Remote repos are cloned in full (bare) into the system temp dir and deleted on exit.

## Analysis options (viewer, `audit`)

| Option | Default | Meaning |
| --- | --- | --- |
| `-b, --branch <REV>` | `HEAD` | Revision to walk. **First-parent history only** |
| `--since <DATE>` / `--until <DATE>` | all | Window, parsed by git (`"2024-01-01"`, `"6 months ago"`). Earlier commits still build up the file state; only in-window commits become samples |
| `-g, --granularity <MODE>` | config, else `auto` | `commit`, `day`, `week`, `month`, `tag`, `auto` |
| `--path <GLOB>` | all | Only analyze matching paths (repeatable) |
| `-e, --exclude <GLOB>` | | Extra excludes (repeatable). Always excluded: `**/node_modules/**`, `**/dist/**`, `**/target/**`, `**/vendor/**`, `**/*.min.js`, plus config `ignore` |

Granularity: a sample is taken at the last commit of each bucket and the graph delta is the **net** change since the previous sample (an edge added and removed inside one bucket is never seen). `auto` uses `commit` if there are at most 300 in-window commits, otherwise the finest of `day`, `week`, `month` that yields at most 300 samples. `tag` samples every tagged commit on the first-parent line, plus the final one.

## Viewer / export

| Option | Default | Meaning |
| --- | --- | --- |
| `--export <FILE>` | serve | `.html` standalone page, `.json` timeline, or `-` for JSON on stdout. `.mp4/.webm/.gif` are rejected |
| `--theme <NAME>` | config, else `github-dark` | `github-dark`, `cyberpunk`, `catppuccin`, `dracula`, `minimal-light` |
| `-o, --overlay <NAME>` | `architecture` | Initial overlay: `architecture`, `hotspots`, `bus-factor`, `cycles`, `orphans` |
| `-p, --port <PORT>` | `4400` | Local server port (bound to `127.0.0.1` only) |
| `--no-open` | | Don't launch a browser |

### Viewer keys

| Key | Action |
| --- | --- |
| `Space` | play / pause |
| `←` `→` or `h` `l` | previous / next sample |
| `Shift+←` `Shift+→` | previous / next tagged sample |
| `+` / `-` | playback speed (0.5x … 100x) |
| `1`…`5` | overlays: architecture, hotspots, bus factor, cycles, orphans |
| `/` | find a file (Enter focuses it) |
| `f` | lock the camera on the selected node |
| `r` | fit the whole graph |
| `q` / `Esc` | clear selection |

Click a node for the inspector (genesis commit, fan-in/out, owner, hotspot score, cycle culprit, imports with the commit that introduced each). Drag to pan, wheel to zoom. The `e` capture shortcut from the spec is not implemented.

## `audit`

Walks history, then reports on HEAD: import cycles (with the commit that closed each), forbidden edges, bus-factor risks, orphans, top-10 hotspots.

| Option | Meaning |
| --- | --- |
| `--format table\|json\|markdown\|sarif` | Output format (default `table`). SARIF 2.1.0 reports cycles, forbidden edges and bus-factor risks |
| `--fail-on-cycles` | exit 1 if any cycle exists at HEAD |
| `--fail-on-forbidden` | exit 1 if a `forbidden_edges` rule in `.chroniclerc.toml` is violated |
| `--max-bus-factor-risk N` | exit 1 if more than `N` files are bus-factor risks |

## `pr-diff <BASE> <HEAD>`

Compares the import graph at the **merge-base** of the two refs (or `BASE` itself if they share no history) against `HEAD`, like GitHub's three-dot PR diff. Reports new/deleted files, `+N edges, -M edges`, cycles that did not exist before, new forbidden edges, and risk warnings for changed files that are single-owner silos or high-churn (5+ changes in the window) single-owner files. Options: `--format markdown|json`, `--fail-on-cycles`, `--fail-on-forbidden`, `--path`, `--exclude`.

GitHub Actions sketch (the tool does not post comments itself):

```yaml
- uses: actions/checkout@v4
  with: { fetch-depth: 0 }
- run: git-chronicle pr-diff origin/${{ github.base_ref }} HEAD --fail-on-cycles > pr-comment.md
- run: gh pr comment ${{ github.event.pull_request.number }} --body-file pr-comment.md
```

## `blame-edge <SOURCE> <TARGET>`

Finds the first commit in which `SOURCE` imports `TARGET`, always at per-commit granularity. Prints the commit, author, message, whether it violated a configured boundary or closed a cycle, and whether the edge still exists at HEAD. Fails with exit 2 if the edge never existed in the analyzed history. Options: `-b/--branch`, `--path`, `--exclude`.
