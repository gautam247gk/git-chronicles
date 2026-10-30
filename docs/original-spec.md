# Technical Specification & Usage Documentation: `git-chronicle`

**Version:** 1.0.0-draft  
**Architecture:** Local-First CLI + Embedded WebGL Visualizer & Terminal UI (TUI)  
**Target Runtime:** Single Static Binary (Rust) + Embedded Web Assets  
**License:** MIT / Apache-2.0  

---

## 1. Executive Summary & Core Value Proposition

`git-chronicle` is a zero-configuration, local-first CLI tool that parses a Git repository’s commit history and Abstract Syntax Tree (AST) import graphs to render an interactive, time-scrubbable visualization of software architecture evolution. 

Unlike **Gource** (which only animates directory trees) or **GitDiagram** (which only renders a static snapshot of `HEAD`), `git-chronicle` maps **actual code dependencies, circular imports, bus-factor risk, and file churn across time**, running at 60 FPS in either an embedded WebGL canvas or a native Terminal UI (TUI), with one-command `.mp4`/`.gif` social export.

---

## 2. System Architecture & Tech Stack

### 2.1 High-Level Pipeline
1. **Git Walker (`git2-rs` / `gix`):** Streams commit objects, trees, and blob diffs directly from `.git/objects` in memory without checking files out to disk.
2. **Incremental AST & Import Extractor (`tree-sitter` + SIMD Regex fallback):** Parses modified blobs per commit to extract `import`, `require`, `#include`, `use`, and `package` edges.
3. **Graph Delta Engine (`petgraph`):** Computes structural graph deltas between commits ($G_t \rightarrow G_{t+1}$), calculating centrality, cycles, and churn metrics in parallel using `rayon`.
4. **Storage & Cache Layer (`redb` or `SQLite`):** Caches computed commit graphs in `.git/chronicle-cache.db` so subsequent runs on 100k+ commit repos load in under 150ms.
5. **Dual Presentation Layer:**
   * **WebGL Mode (Default):** Embedded `axum` localhost server serving a single-bundle WebGL/Canvas app (`PixiJS` / `Three.js` + `d3-force`) packed into the binary via `rust-embed`.
   * **Terminal Mode (`--tui`):** Native terminal rendering using `ratatui` with Braille/Canvas force-directed graph plotting and live metric sparklines.
6. **Headless Video Encoder:** Renders WebGL frames or custom `tiny-skia` 2D frames directly into an embedded `minimp4` / `gif` encoder or pipes raw RGBA frames to `ffmpeg` if installed.

---

## 3. Complete Feature Specification

### Feature 1: Incremental AST Dependency Graph Engine
Instead of visualizing folders, `git-chronicle` builds a directed graph $G = (V, E)$ for every sampled commit:
* **Vertices ($V$):** Source files, external packages, or logical modules.
* **Edges ($E$):** Explicit dependency links (`File A` imports `File B`).
* **Incremental Diff Parsing:** At commit $C_n$, only blobs whose SHA-1/SHA-256 changed since $C_{n-1}$ are re-parsed. Unchanged files inherit their dependency edges from $C_{n-1}$.
* **Supported Languages Out-of-the-Box:**
  * **TypeScript / JavaScript:** ES6 `import`, dynamic `import()`, CommonJS `require()`, `tsconfig.json` path aliases.
  * **Rust:** `mod`, `use crate::`, workspace `Cargo.toml` crate links.
  * **Python:** `import`, `from ... import` (relative and absolute module resolution).
  * **Go:** `import` declarations mapped against `go.mod`.
  * **C / C++ / Java / C#:** `#include`, `import`, `using`.

### Feature 2: Interactive Time-Scrubbing & Playback Engine
* **Non-Linear Timeline Scrubber:** Scrub smoothly across 10+ years of commits with instant graph morphing (spring-physics interpolation between commit states).
* **Adaptive Commit Sampling:** For massive repos (e.g., Linux, React), automatically buckets commits by day, week, or release tag (`--granularity=commit|day|week|tag`) while preserving exact state accuracy.
* **Playback Controls:** Play/Pause, variable speed (`0.5x` to `100x`), step forward/backward by single commit (`[` and `]`), and jump to Git tags/releases.
* **Commit Event Ticker:** Displays commit messages, author avatars, and lines added/removed (`+LOC / -LOC`) synced to the timeline.

### Feature 3: The "Tech Debt & Risk Radar" (Visual Overlays)
Users can switch between 5 node/edge color-coding modes in real time:

| Overlay Mode | Visual Encoding | Exact Mathematical Formula / Trigger |
| :--- | :--- | :--- |
| **1. Architecture (Default)** | Nodes colored by top-level module/directory; sized by Lines of Code (LOC). | $\text{Radius} = \max(4, \log_2(\text{LOC}) \times k)$ |
| **2. Churn / Hotspot Radar** | Nodes glow from Cool Blue $\rightarrow$ Neon Red based on modification frequency inside a rolling window (default: 90 days). | $\text{Hotspot Score} = \text{Churn}_{90d} \times \log_{10}(\text{LOC}) \times \text{FanIn}$ |
| **3. Bus Factor & Knowledge Silo** | Highlights critical files (high Fan-In) where $>80\%$ of lines were authored by a single developer or an inactive contributor. | $\text{Silo Risk} = \text{True if } (\text{AuthorShare}_{\max} > 0.8) \land (\text{FanIn} \ge 5)$ |
| **4. Circular Dependency Detector** | Pulses edges in bright magenta whenever a commit introduces a strongly connected component (cycle) using Tarjan's algorithm. | $SCC(G) \text{ where } |V_{scc}| > 1$ |
| **5. Dead Weight / Orphan Finder** | Dims or highlights internal modules with zero incoming edges (`Fan-In == 0`) that are not configured entrypoints. | $\text{deg}^-(v) = 0 \land v \notin \text{Entrypoints}$ |

### Feature 4: "Who Broke the Architecture?" Blame Inspector
Clicking any file node or dependency edge opens the **Chronicle Inspector Panel**:
* **Genesis Commit:** The exact commit and author that created the file or introduced the dependency edge.
* **Cycle Culprit:** If an edge is part of a circular dependency, displays the exact commit SHA, date, PR number, and commit message that closed the loop.
* **Coupling History:** Shows the top 5 files that most frequently change in the same commit as the selected file (Temporal Coupling / Co-change matrix).

### Feature 5: Zero-Config Viral Video & GIF Exporter (`--export`)
Designed specifically for sharing on X, Reddit, Hacker News, and LinkedIn, or embedding in pull requests:
* **Preset Aspect Ratios:**
  * `landscape` ($1920 \times 1080$ at 60 FPS) for YouTube, X, and README headers.
  * `square` ($1080 \times 1080$ at 60 FPS) for LinkedIn and blogs.
  * `vertical` ($1080 \times 1920$ at 60 FPS) for TikTok, Reels, and Shorts.
* **Automated Camera Director:** Automatically pans and zooms to follow the highest-activity cluster at any point in the timeline so the video feels cinematically directed rather than static.
* **HUD Overlay Options:** Optional sleek overlays showing a live counter of Total Files, Total LOC, Current Date, Release Tags, and a subtle customizable badge.

### Feature 6: CI/CD PR Architecture Diff Mode (`--pr-diff`)
Can run headlessly in GitHub Actions on every Pull Request:
* Compares `base` branch against `head` branch.
* Generates a static SVG/GIF architecture delta showing newly added dependencies, broken module boundaries, and any newly introduced circular dependencies.
* Posts a markdown summary table directly to the PR comment.

---

## 4. Data Schema & Configuration

### 4.1 `.chroniclerc.toml` (Optional Repository Configuration)
While `git-chronicle` requires **zero configuration** to run, teams can drop a `.chroniclerc.toml` file in their repo root to customize rules and visual themes:

```toml
[project]
name = "Acme Core Engine"
entrypoints = ["src/main.rs", "src/lib.rs", "packages/web/src/index.tsx"]
ignore = ["**/node_modules/**", "**/dist/**", "**/*.test.ts", "**/vendor/**"]

[analysis]
granularity = "commit" # "commit" | "day" | "week" | "tag"
hotspot_window_days = 90
inactive_author_days = 180 # Marks authors with no commits in 180d as "departed" for Bus Factor

[architecture.boundaries]
# Alert if "ui" ever imports directly from "database"
forbidden_edges = [
  { from = "src/ui/**", to = "src/database/**" }
]

[export]
theme = "cyberpunk" # "cyberpunk" | "github-dark" | "catppuccin" | "minimal-light"
resolution = "1080p"
fps = 60
duration_seconds = 20
show_hud = true
```

---

## 5. Complete CLI Usage Documentation

### 5.1 Installation

```bash
# Run immediately without installing (Node/npx wrapper)
npx git-chronicle

# Install via Homebrew (macOS / Linux)
brew install git-chronicle

# Install via Cargo (Rust)
cargo install git-chronicle

# Install via Scoop / Winget (Windows)
winget install git-chronicle
```

---

### 5.2 Quickstart Commands

```bash
# 1. Analyze current repository and open the interactive WebGL UI in browser
git-chronicle

# 2. Launch in native Terminal UI (TUI) mode (no browser required, great for SSH)
git-chronicle --tui

# 3. Export a 15-second 1080p MP4 timelapse of the entire repo history
git-chronicle --export chronicle.mp4 --duration 15

# 4. Analyze a specific subdirectory or monorepo package over the last 6 months
git-chronicle --path packages/api --since "6 months ago"

# 5. Audit HEAD for circular dependencies and bus-factor risks (exits non-zero for CI)
git-chronicle audit --fail-on-cycles --max-bus-factor-risk 3
```

---

### 5.3 Full Command-Line Reference

#### Synopsis
```text
git-chronicle [SUBCOMMAND] [OPTIONS] [REPO_PATH]
```

#### Global Options
| Flag / Option | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `<REPO_PATH>` | | `.` | Path to the local Git repository or remote Git URL (clones shallow blobless history into temp dir). |
| `--tui` | `-t` | `false` | Run in interactive Terminal UI mode instead of launching the local WebGL browser server. |
| `--port <PORT>` | `-p` | `4400` | Port for the local WebGL server. |
| `--no-open` | | `false` | Start the local server without automatically opening the system browser. |
| `--branch <BRANCH>` | `-b` | `HEAD` | Target branch or revision range (e.g., `main`, `v1.0..v2.0`). |
| `--since <DATE>` | | *First Commit* | Start analysis from a specific date or relative time (`"2024-01-01"`, `"1 year ago"`). |
| `--until <DATE>` | | `HEAD` | End analysis at a specific date or commit ref. |
| `--granularity <MODE>` | `-g` | `auto` | Commit sampling mode: `auto`, `commit`, `day`, `week`, `month`, or `tag`. |
| `--path <GLOB>` | | `*` | Filter analysis to specific paths or globs (can be passed multiple times). |
| `--exclude <GLOB>` | `-e` | *Standard Ignores* | Exclude files or directories matching glob pattern (respects `.gitignore` by default). |
| `--overlay <MODE>` | `-o` | `architecture` | Initial visual mode: `architecture`, `hotspots`, `bus-factor`, `cycles`, `orphans`. |
| `--no-cache` | | `false` | Bypass `.git/chronicle-cache.db` and recompute all AST graphs from scratch. |
| `--threads <N>` | `-j` | *All Cores* | Number of CPU worker threads for parallel Git & Tree-sitter parsing. |

---

#### Export Options (`--export`)
| Flag / Option | Default | Description |
| :--- | :--- | :--- |
| `--export <FILE>` | *None* | Output file path. Format inferred from extension: `.mp4`, `.webm`, `.gif`, or `.json`. |
| `--duration <SECONDS>` | `15` | Total length of the exported timelapse video in seconds. |
| `--fps <FPS>` | `60` | Frame rate for video export (`24`, `30`, `60`). |
| `--preset <PRESET>` | `landscape` | Aspect ratio preset: `landscape` (1920x1080), `square` (1080x1080), `vertical` (1080x1920). |
| `--theme <THEME>` | `github-dark` | Color palette: `github-dark`, `cyberpunk`, `catppuccin`, `dracula`, `minimal-light`. |
| `--camera <MODE>` | `auto-director` | Camera behavior: `auto-director` (follows action), `fit-all` (static zoom to fit), `focus:<path>`. |
| `--hide-authors` | `false` | Anonymize author names and avatars in the video HUD (useful for enterprise repos). |

---

#### Subcommands

##### 1. `git-chronicle audit`
Runs a headless static + historical check on the repository and prints a rich terminal report (or JSON for CI pipelines).
```bash
git-chronicle audit [OPTIONS]
```
* `--fail-on-cycles`: Exit with code `1` if any circular dependencies exist at `HEAD`.
* `--fail-on-forbidden`: Exit with code `1` if any `forbidden_edges` in `.chroniclerc.toml` are violated.
* `--format <FMT>`: Output format (`table`, `json`, `markdown`, `sarif`).

##### 2. `git-chronicle pr-diff <BASE_REF> <HEAD_REF>`
Compares architectural graph differences between two refs.
```bash
git-chronicle pr-diff origin/main HEAD --format markdown > pr-comment.md
```
Outputs:
* New modules and deleted modules.
* Net dependency edge changes (`+14 edges, -3 edges`).
* New circular dependency loops introduced by the branch.
* Hotspot warnings if the PR modifies high-churn, single-owner files.

##### 3. `git-chronicle blame-edge <SOURCE_FILE> <TARGET_FILE>`
Finds the exact historical commit where `<SOURCE_FILE>` first imported `<TARGET_FILE>`.
```bash
$ git-chronicle blame-edge src/auth/session.ts src/db/raw_client.ts

Edge Introduced in Commit: a8f921c (2025-11-14)
Author: Alex Rivera <alex@example.com>
Commit Message: "feat(auth): quick fix for session lookup timeout"
Impact: Created a direct coupling between UI Auth layer and Raw DB Client (+1 boundary violation).
```

---

### 5.4 Interactive Keyboard Shortcuts (WebGL & TUI Modes)

| Keybind | Action |
| :--- | :--- |
| `Space` | Play / Pause timeline playback |
| `←` / `→` or `h` / `l` | Step backward / forward 1 commit |
| `Shift + ←` / `Shift + →` | Jump to previous / next Git Tag or Release |
| `+` / `-` | Increase / decrease playback speed (`0.5x` $\rightarrow$ `100x`) |
| `1` | Switch to **Architecture** overlay |
| `2` | Switch to **Churn / Hotspot** overlay |
| `3` | Switch to **Bus Factor / Silo** overlay |
| `4` | Switch to **Circular Dependency** overlay |
| `5` | Switch to **Orphan / Dead Code** overlay |
| `/` | Fuzzy-search and focus camera on a specific file or module |
| `f` | Toggle camera lock on currently selected node |
| `r` | Reset camera zoom and pan to fit entire graph |
| `e` | Trigger instant `.gif` / `.mp4` capture of current time range |
| `q` / `Esc` | Clear selection / Quit TUI |

---

## 6. Phased Implementation Roadmap (MVP to Viral v1.0)

1. **Milestone 1 (Weekend Prototype — Core Engine):**
   * Build the Rust CLI with `clap`, `git2`, and `rayon`.
   * Implement fast AST/regex import extraction for TypeScript/JavaScript, Python, and Rust.
   * Output a serialized timeline JSON payload (`commits[]` + `graph_deltas[]`).
2. **Milestone 2 (The Visual Hook — WebGL UI):**
   * Build the embedded single-page WebGL visualizer using PixiJS / Force-Graph.
   * Add the time scrubber, spring physics transitions, and the 5 color overlays (Architecture, Hotspots, Bus Factor, Cycles, Orphans).
3. **Milestone 3 (The Viral Multiplier — Export & Polish):**
   * Add the `--export` flag to render 60 FPS `.mp4` and `.gif` timelapses with the HUD counter.
   * Record 3 showcase videos on iconic repositories (`facebook/react`, `torvalds/linux`, `denoland/deno`) for the GitHub README header.