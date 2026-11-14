# Contributing to Argus

Thanks for your interest! Argus is in early development, so issues, ideas and pull
requests are all welcome. All discussion, code comments and commits are in English.

## Development

```sh
cargo run -- --demo        # run with sample data
cargo test                 # run the test suite
cargo fmt --all            # format
cargo clippy --all-targets -- -D warnings
```

CI runs the same checks on Linux, macOS and Windows for every pull request.

## Project layout

```
src/
  main.rs        CLI and event loop
  app.rs         application state, views, key handling
  i18n.rs        localization (Project Fluent)
  model.rs       agents, hosts and snapshots shared by all views
  ui/            one module per view
locales/         translations, one .ftl file per language
docs/            design documents
```

## Adding a translation

1. Copy `locales/en.ftl` to `locales/<code>.ftl` (for example `de.ftl`).
2. Translate the values; keep message ids and `{ $placeholders }` unchanged.
3. Register the file in the `LOCALES` list in `src/i18n.rs`.
4. Run `cargo test` — a test fails if any message is missing.
5. Try it with `cargo run -- --demo --lang <code>` and open a pull request.

## Commit style

Short, imperative subject lines (e.g. `Add Graph view`, `Fix context bar rounding`).
