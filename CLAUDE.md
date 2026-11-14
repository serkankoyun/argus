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
1. Bump `version` in `Cargo.toml`, update docs, push to `main`, wait for green CI.
2. The maintainer publishes a release in the GitHub UI with a new `vX.Y.Z` tag (Claude
   sessions cannot create tags or releases).
3. The **Release** workflow builds 5 targets, attaches archives + `SHA256SUMS`, then
   regenerates `bucket/argus.json` and `Formula/argus.rb` (`packaging/render.py`) on `main`.
4. Run the **Packages** workflow (`gh workflow run packages.yml`) to install via Homebrew and
   Scoop and verify the manifests (bot commits do not trigger workflows on their own).
- Re-attach binaries to an existing release: run the Release workflow manually with `tag`.

## Lessons learned
- **Windows terminals and glyph widths**: symbols with emoji variants (▶ ◀) or ambiguous width
  (▰ ▱ ⏎) can be drawn two cells wide and shift ratatui's diffed output ("fields being erased").
  Use narrow glyphs (► ◄ █ ░, text like `enter`) and keep `terminal.clear()` on view changes.
- **Windows key events**: crossterm reports press *and* release; act on everything except
  `KeyEventKind::Release`. Offer alternatives (F-keys, mouse) and `--debug-keys` for diagnosis.
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
- **GitHub releases API**: `/releases/latest` ignores pre-releases; use `/releases?per_page=1`.
- **Release packaging**: build archives in `dist/` and upload `dist/*` only; staging folder names
  contain dots and globbing `argus-*.*` picked them up.
- **Automation limits**: Claude sessions cannot push tags, create releases or change repo
  settings; workflows pushed by `GITHUB_TOKEN` do not trigger other workflows.
- **Package updates**: `scoop update <app>` needs `scoop update` first; quit Argus before
  updating on Windows.
- **Name clash**: winget's `argus` packages are unrelated; reserve `SerkanKoyun.Argus`.

## Layout
- `src/source/` — data sources: `claude.rs` (live `~/.claude` collector), `demo.rs`
  (scripted simulation), `system.rs` (host metrics, running `claude` processes), `pricing.rs`,
  `events.rs` (lifecycle events), `hook.rs` (`argus hook`)
- `src/model.rs` — `Snapshot`, `Agent`, `Message`, `ToolCall`, `Task`, `Host`, `Metrics`
- `src/app.rs` — state, views, keys; `src/ui/` — one module per view
