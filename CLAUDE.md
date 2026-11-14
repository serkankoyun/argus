# Argus — notes for Claude sessions

Read `docs/PROJECT_BRIEF.md` first: it holds the vision, every decision and the roadmap.

## Rules
- Everything in the repository is in **English** (code, comments, docs, commits, issues).
  The maintainer talks to Claude in Turkish; the app UI is multilingual.
- Every user-visible string goes through `I18n` (`locales/*.ftl`). Add each new key to
  **all** locale files — `cargo test` fails otherwise.
- Argus must always work **standalone on the local machine** with no hub, SSH or network.
  Remote hosts are an optional extra.
- The collector must parse leniently: unknown fields are ignored, missing ones degrade.
- **Every change updates all related docs in the same commit**: `README.md`,
  `docs/INSTALL.md`, `packaging/README.md` (shipped inside release archives),
  `docs/PROJECT_BRIEF.md` (decisions, status, roadmap) and this file when workflows change.
- **Every problem the maintainer or a user hits is written down** once solved: user-facing
  fixes in `docs/INSTALL.md` → Troubleshooting (symptom first, then steps), developer lessons
  in "Lessons learned" below.
- Keep the classic terminal aesthetic of the design mockups: no emoji, ratatui borders,
  braille/box characters, the shared palette in `src/ui/common.rs`.

## Workflow
- The Claude cloud workspace may have no network access to crates.io. GitHub Actions is then
  the compile check: push, then `gh run watch <id> -R serkankoyun/argus --exit-status`.
  CI runs fmt, `clippy -D warnings`, tests and a release build on Linux, macOS and Windows.
- Visual check: the **Demo media** workflow records `docs/media/*.png` and GIFs with vhs from
  `argus --demo` (`docs/tapes/`). Pull and look at the PNGs after UI changes.
- Format with `rustfmt --edition 2021 src/main.rs` before committing.
- Commit style: short imperative subject; body explains why.

## Releasing
The maintainer opted in to automatic releases (Nov 14, 2026):
1. Bump `version` in `Cargo.toml`, update docs, push to `main`.
2. When CI is green, **Auto release** (`autorelease.yml`) creates the `vX.Y.Z` pre-release
   and dispatches **Release**, which builds 5 targets, attaches archives + `SHA256SUMS`,
   regenerates `bucket/argus.json` and `Formula/argus.rb` (`packaging/render.py`) on `main`
   and dispatches **Packages** to verify Homebrew and Scoop installs.
3. Check the runs (`gh run list -R serkankoyun/argus --limit 5`), then tell the maintainer
   the version is out. A version that already has a tag is never released twice.
- Re-attach binaries to an existing release: run the Release workflow manually with `tag`.

## Lessons learned
- **Windows terminals and glyph widths**: symbols with emoji variants (▶ ◀) or ambiguous width
  (▰ ▱ ⏎) can be drawn two cells wide and shift ratatui's diffed output ("fields being erased").
  Use narrow glyphs (► ◄ █ ░, text like `enter`) and keep `terminal.clear()` on view changes.
- **Windows key events**: crossterm reports press *and* release; act on everything except
  `KeyEventKind::Release`. Some terminals report Shift+letter as the lowercase letter with
  `SHIFT` (`T` cycled teams like `t`); `App::on_key` normalizes that. Offer alternatives (F-keys, mouse) and `--debug-keys` for diagnosis.
- **Never write outside the buffer**: custom-drawn views must use `put`/`putn` from
  `src/ui/common.rs`, never `Buffer::set_string` directly — ratatui panics on out-of-bounds
  writes, and real data (dozens of sessions) overflows layouts that the demo never fills.
- **Child processes and the terminal**: every spawned process (`ssh`, `claude`) must get
  `stdin(Stdio::null())` unless it is fed through a pipe. An inherited stdin lets the child read
  the console and steal the TUI's key presses (seen on Windows with `--host`: keys worked only
  in `--demo`).
- **Cross-platform remotes**: a Windows binary cannot be copied to a Linux server. The remote
  downloads its own build (`download_script` in `src/source/remote.rs`). Non-interactive SSH
  shells often lack `~/.local/bin` or Homebrew on `PATH`; probe known install paths. Run remote
  scripts through `sh -c` so fish/zsh login shells do not break them.
- **Remote collector versions**: never reuse a remote argus of another version; the probe
  matches `argus <this version>` exactly and otherwise reinstalls. An old cached collector
  silently showed stale or missing data after local updates.
- **Transcripts lag**: Claude Code writes a transcript line only when a step finishes; a long
  `Bash` looked like a pending approval and an idle session looked busy. Exact states come
  from lifecycle events (`src/source/events.rs`, `docs/EVENTS.md`); keep that format open and
  generic rather than adding code for one orchestrator.
- **Liveness is by folder prefix**: Claude Code keeps the launch folder as its process cwd
  while sessions `cd` into subprojects, and teammates may run inside the lead's process. An
  exact cwd match marked live sessions finished, and hiding finished agents then emptied
  the views. Real data never looks like the demo: test with nested folders.
- **GitHub releases API**: `/releases/latest` ignores pre-releases; use `/releases?per_page=1`.
- **Release packaging**: build archives in `dist/` and upload `dist/*` only; staging folder names
  contain dots and globbing `argus-*.*` picked them up.
- **Automation limits**: Claude sessions cannot push tags, create releases or change repo
  settings. Events made with `GITHUB_TOKEN` start no other workflows except
  `workflow_dispatch`, so automation chains dispatch the next workflow explicitly.
- **Package updates**: `scoop update <app>` needs `scoop update` first; quit Argus before
  updating on Windows.
- **Name clash**: winget's `argus` packages are unrelated; reserve `SerkanKoyun.Argus`.

## Layout
- `src/source/` — data sources: `claude.rs` (live `~/.claude` collector), `demo.rs`
  (scripted simulation), `system.rs` (host metrics, running `claude` processes), `pricing.rs`,
  `events.rs` (lifecycle events), `hook.rs` (`argus hook`), `groups.rs` (team rules)
- `src/model.rs` — `Snapshot`, `Agent`, `Message`, `ToolCall`, `Task`, `Host`, `Metrics`
- `src/app.rs` — state, views, keys; `src/ui/` — one module per view
