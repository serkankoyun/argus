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
4. The **Packages** workflow installs via Homebrew and Scoop to verify the manifests.
- Re-attach binaries to an existing release: run the Release workflow manually with `tag`.

## Layout
- `src/source/` — data sources: `claude.rs` (live `~/.claude` collector), `demo.rs`
  (scripted simulation), `system.rs` (host metrics, running `claude` processes), `pricing.rs`
- `src/model.rs` — `Snapshot`, `Agent`, `Message`, `ToolCall`, `Task`, `Host`, `Metrics`
- `src/app.rs` — state, views, keys; `src/ui/` — one module per view
