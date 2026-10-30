# Limitations

Know these before trusting the numbers.

**Import extraction is heuristic.** Imports are found with regular expressions after stripping whole-line and block comments, not with a parser (the spec's tree-sitter is not used). Consequences:
- Imports inside strings, template literals, or `#[cfg]`-gated / conditional code are treated like any other import. Trailing `//` comments on a code line are not stripped.
- TS/JS: only relative specifiers become edges. `tsconfig.json` path aliases, package `exports`, and bare workspace-package imports are unresolved (they are treated as external).
- Python: absolute imports are matched by dotted-path suffix against repository files (`import a.b` → any `…/a/b.py` or `…/a/b/__init__.py`); on ambiguity the candidate closest to the importer wins. Dynamic imports (`importlib`) are invisible.
- Rust: `mod foo;`, and `use crate::…`, `self::…`, `super::…` resolve to files. Inline `mod foo { }`, `#[path]`, nested `use` groups deeper than one level, `use` of other workspace crates, and `src/bin/*` crate roots are not handled.
- Go, C, C++, Java, C# files are ignored entirely.

**History model.** Only first-parent history is walked; merge commits are diffed against their first parent, so merged branches appear as one sample at the merge. Renames are treated as delete + add. Commits that are not on the first-parent line are not sampled, and tags on them are ignored. Paths git has to quote (containing `"`, `\`, tabs or newlines) and submodules are skipped. Files over 2 MB or not valid UTF-8 are counted as 0 LOC with no imports.

**Sampling.** Between two samples only the net change is visible. Use `-g commit` (always the case for `blame-edge`) for exact attribution. Each sample re-resolves every file's imports, so `-g commit` on very large repositories is slow; `auto` caps at 300 samples. The whole first-parent log is held in memory.

**Metrics.** Churn, author share, bus-factor, hotspot and orphan flags are computed at HEAD and shown unchanged when scrubbing in the viewer. Author share counts lines added, not surviving lines. No temporal-coupling (co-change) analysis is implemented. Authors are identified by name, not e-mail, so one person with two names counts as two.

**Viewer.** 2D canvas, not WebGL; no spring-physics morphing between commits; practical up to a few thousand visible files. It was exercised with a stub-DOM script under Node and by inspecting the generated HTML, not in an automated real-browser test. The page has no video/GIF capture.

**Not implemented:** `--tui`, mp4/webm/gif export and its presets/camera/HUD flags, `--hide-authors`, persistent cache and `--no-cache`/`--threads`, SVG/GIF PR diff, posting PR comments, remote clones with shallow/blobless filters (full bare clone only), configuration for remote-URL clones, package-manager distribution.

**Platform.** Developed and tested on Linux; CI also runs macOS. Windows is untested. Requires `git` on `PATH`; output of `git` is parsed with `core.quotepath=off` and is assumed to be UTF-8.
