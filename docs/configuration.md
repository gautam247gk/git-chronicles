# Configuration

Configuration is optional. Drop a `.chroniclerc.toml` in the repository's working directory root. It is read from the working tree (not from a git ref), so it is not available for remote-URL clones.

```toml
[project]
name = "Acme Core Engine"                  # shown in the viewer title
entrypoints = ["src/main.rs", "packages/web/src/index.tsx"]   # globs; exempt from orphan detection
ignore = ["**/*.test.ts", "**/generated/**"]                  # globs, added to the built-in ignores

[analysis]
granularity = "commit"       # auto | commit | day | week | month | tag   (CLI -g overrides)
hotspot_window_days = 90     # churn window ending at the last analyzed commit
inactive_author_days = 180   # top author silent this long => "owner inactive"

[architecture.boundaries]
forbidden_edges = [
  { from = "src/ui/**", to = "src/database/**" },   # glob on importer, glob on imported file
]

[export]
theme = "cyberpunk"          # github-dark | cyberpunk | catppuccin | dracula | minimal-light
```

Globs use [globset](https://docs.rs/globset) syntax and match repo-relative paths with `/` separators.

Defaults when a key is absent: `granularity = "auto"`, `hotspot_window_days = 90`, `inactive_author_days = 180`, `theme = "github-dark"`.

Unknown keys are ignored, so the spec's `[export] resolution/fps/duration_seconds/show_hud` parse but have **no effect** (video export is not implemented). A malformed file is an error that names the file.

Conventional entrypoints are always exempt from orphan detection, with or without config: files named `main.rs lib.rs build.rs index.{ts,tsx,js,jsx} main.{ts,js} __init__.py __main__.py setup.py conftest.py`, `test_*` files, `*.test.*` / `*.spec.*` files, and anything under `test/ tests/ __tests__/ spec/ examples/ example/`.
