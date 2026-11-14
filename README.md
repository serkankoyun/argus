# Argus

**Watch and steer your Claude Code agents live, from the terminal, on any machine.**

Argus is a terminal UI that shows what every Claude Code agent is doing *right now*:
which tool it is running, how much context and money it is burning, and — above all —
**how agents hand off tasks and talk to each other**. Connect to remote machines over SSH
and see all of your agents, on every host, in one place.

> **Status:** early development — all ten views work with the live collector and the demo.
> Star the repo to follow along.

![Argus demo](docs/media/demo-en.gif)

## Why Argus?

Existing tools pick one side: cozy pixel-art offices that look great but tell you little,
or `htop`-style tables that tell you numbers but not the story. Argus keeps a classic
terminal aesthetic and gives you both — full metrics *and* a live picture of the
conversation between your agents.

## Features (planned for v1.0)

- **Teams** — agents grouped by Agent Teams team (or project); one card per team, cross-team requests, and `t` filters every view to one team
- **Ten views, one keystroke apart** — switch instantly with `1`–`0` or `tab`; the selected agent follows you
- **Live agent graph** — messages travel between agents in real time; edge width shows traffic
- **"Who asks whom for what"** — every open request, who is waiting on whom, and for how long
- **Full metrics** — tokens, cache hits, context window, cost, throughput, tool calls, files touched
- **Remote machines over SSH** — `:connect prod-1` and the collector installs itself; no ports to open
- **Drive your agents** — start new Claude Code runs, message running agents, approve permission requests
- **Multilingual UI** — English and Turkish today; translations welcome
- **Cross-platform** — a single binary for macOS, Linux and Windows

## Views

| Overview | Live graph |
|---|---|
| ![Overview](docs/media/overview-en.png) | ![Graph](docs/media/graph-en.png) |
| **Room** | **Lanes** |
| ![Room](docs/media/room-en.png) | ![Lanes](docs/media/lanes-en.png) |
| **Task map** | **Pair** |
| ![Task map](docs/media/taskmap-en.png) | ![Pair](docs/media/pair-en.png) |


| # | View | What it shows |
|---|---|---|
| 1 | Overview | Hosts, agent tree, metrics, event log |
| 2 | Sequence | Agents as lanes, messages as arrows, task board |
| 3 | Agent | Context breakdown, cost, tool calls, live transcript |
| 4 | Fleet | Every connected host, collector health, alerts |
| 5 | Graph | Live network of agents with messages in flight |
| 6 | Room | What each agent is doing now and what it is asking for |
| 7 | Lanes | Scrolling per-agent timelines with cross-agent messages |
| 8 | Channel | IRC-style live chat between agents |
| 9 | Task map | Task dependencies, hand-offs and bottlenecks |
| 0 | Pair | Two agents side by side, their conversation in between |

## Install

Download a binary for macOS, Linux or Windows from the
[latest release](https://github.com/serkankoyun/argus/releases), or build from source
with a recent stable Rust toolchain:

```sh
git clone https://github.com/serkankoyun/argus
cd argus
cargo run --release -- --demo
```

## Usage

```sh
argus                      # watch agents on this machine
argus --demo               # explore the UI with sample data
argus --lang tr            # force a UI language (default: system locale)
argus --claude-dir <DIR>   # read another Claude config dir (default: ~/.claude)
```

| Key | Action |
|---|---|
| `1`–`0` | Jump to a view |
| `tab` / `shift+tab` | Next / previous view |
| `↑` `↓` / `j` `k` | Select agent (the selection follows you across views) |
| `⏎` / `esc` | Agent detail / back to Overview |
| `z` | Zoom the Lanes timeline |
| `t` / `T` | Filter all views to a team / open the Teams view |
| `g` | Group Graph and Room by team instead of host |
| `a` / `r` | Approve / reject the pending permission request |
| `:` | Command bar: `connect user@host`, `msg <text>`, `run <dir> <prompt>`, `view <name>`, `lang tr` |
| `>` | Start a new headless Claude Code run in the selected agent's project |
| `space` | Pause / resume live updates |
| `q` | Quit |

## Remote machines

```sh
argus --host me@prod-1 --host me@gpu-2   # or press : and type connect me@prod-1
```

Argus uses your own `ssh` (keys, config, jump hosts all work). If the remote has no
`argus`, the local binary is copied to `~/.cache/argus/` when the platforms match.
Nothing listens on a port; the collector stops when you disconnect.

## Approving from Argus

```sh
argus hook --install    # adds a PermissionRequest hook to ~/.claude/settings.json
```

Claude Code then asks Argus first: press `a` or `r`. If nobody answers within
`--wait` seconds (default 120) the normal permission prompt appears.

## Roadmap

- **v0.1** ✓ — local collector, Overview, agent tree, metrics, EN + TR, view switching
- **v0.2** ✓ — Graph, Room, Lanes, Agent, Sequence, Fleet, Channel, Task map, Pair views
- **v0.3** ✓ — remote machines over SSH, Fleet view, Windows support
- **v0.4** ✓ — command bar, launching agents, remote approvals, teams
- **v1.0** — Channel, Task map and Pair views, replay, packages

See [`docs/PROJECT_BRIEF.md`](docs/PROJECT_BRIEF.md) for the full design.

## Contributing

Contributions are welcome — especially translations. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

[MIT](LICENSE)

---

*Argus is an independent project and is not affiliated with or endorsed by Anthropic.
"Claude" and "Claude Code" are trademarks of Anthropic.*
