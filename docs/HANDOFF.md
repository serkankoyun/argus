# Handoff: developing Argus on the machine that runs the agents

A Claude Code session on the agents' own machine can do what a cloud session cannot:
build Argus against the real `~/.claude` data, look at every view itself, compare it with
what the coordinator knows, and ship the fix. This file is the starting point for such a
session. Read `CLAUDE.md` and `docs/PROJECT_BRIEF.md` first.

## The loop

1. **Build:** `cargo build --release` (install Rust once with `curl https://sh.rustup.rs -sSf | sh -s -- -y`).
2. **Look at the screens** in a detached tmux session; never guess what a view shows:
   ```sh
   tmux new-session -d -s argus-view -x 200 -y 56 './target/release/argus --lang tr'
   tmux send-keys -t argus-view t            # switch view (1-0, t, H, f, s ...)
   sleep 2; tmux capture-pane -p -t argus-view   # read the screen as text
   ```
   Restart the tmux session after each build. `argus snapshot --wait 3` gives the same data as
   JSON for exact checks (state, team, role, parent, task per agent).
3. **Compare with reality:** ask the coordinator session what is really running (cross-session
   messaging, or the maintainer relays). Its view is the ground truth; Argus must match it.
4. **Fix** with a test that reproduces the real case (the shape of the real transcript lines,
   not the demo). `cargo test`, `cargo clippy --all-targets -- -D warnings`,
   `rustfmt --edition 2021 src/main.rs`.
5. **Ship:** bump `version` in `Cargo.toml`, update the docs listed in `CLAUDE.md`, commit,
   `git push`. CI and **Auto release** publish the version; the maintainer updates with
   `scoop update argus` (Windows) and the collector on this machine updates itself on the
   next connection.
6. **Close the loop:** comment on and close the GitHub issue with evidence from step 2.

## Real data rules learned so far

- Subagents of a session are stored flat (`<session>/subagents/agent-<id>.jsonl`); the real
  parent is the agent whose spawn result returned the child's `agentId`
  (`toolUseResult.agentId` on the transcript line, or the result text).
- `SubagentStop` fires whenever a subagent's turn ends, including while it waits on a
  background job or on agents it started; it is not "finished". A subagent without a
  hand-back whose children still run is waiting (`[wait …]`), and a `Stop` event does not
  make a `[bg …]`/`[wait …]` agent idle (v0.1.27).
- `subagents/agent-<id>.meta.json` holds `name`, `description`, `parentAgentId`,
  `spawnDepth` and `finished`; `finished: true` is written when a turn ends, so it is
  true for a lead that still waits on its children.
- Orchestrator descriptions follow `<agent-name> · <run> · <task>`; nested agents have no
  `name` parameter.
- Team rules live in `~/.claude/argus/teams.json` (see `docs/INSTALL.md`); `argus init` shows
  what the rules make of today's names.

## Open work (see GitHub issues)

- #5 / #9: verified on live data with v0.1.27 (waiting leads and coordinators, nested
  parents, names, teams).
- #10: Network "Independent" cluster and lead→child edges, Room team tables, and the
  **Görev haritası** redesign: map runs (`runs/<run-id>/run.json` and stage files) with stage
  chips Analiz → Plan → Geliştirme → Test → QA → Güvenlik → CI → Birleşme and a bottleneck
  panel, instead of agents.

## Access this session needs

- Push to `serkankoyun/argus`: a fine-grained token with **Contents: Read and write** and
  **Issues: Read and write** on that repository only (`gh auth login --with-token`).
- Read access to `~/.claude` and the projects' `runs/` folders (it runs as the same user).
- It must not change other projects or the orchestrator; Argus files only.
