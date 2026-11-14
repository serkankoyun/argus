# Installing and setting up Argus

This guide covers installation on macOS, Linux and Windows, the first run, optional
features (approvals from Argus, remote machines over SSH), troubleshooting and removal.

- [Requirements](#requirements)
- [1. Install](#1-install)
  - [With a package manager (recommended)](#with-a-package-manager-recommended)
  - [macOS](#macos)
  - [Linux](#linux)
  - [Windows](#windows)
  - [Build from source](#build-from-source)
- [2. First run](#2-first-run)
- [3. Approving tool calls from Argus (optional)](#3-approving-tool-calls-from-argus-optional)
- [4. Watching remote machines over SSH (optional)](#4-watching-remote-machines-over-ssh-optional)
- [5. Starting and steering agents](#5-starting-and-steering-agents)
- [6. Troubleshooting](#6-troubleshooting)
- [7. Updating and uninstalling](#7-updating-and-uninstalling)

## Requirements

- [Claude Code](https://code.claude.com) installed on every machine you want to watch.
  Argus reads the session files Claude Code writes under `~/.claude` (or `$CLAUDE_CONFIG_DIR`).
- A terminal with Unicode and 256-color or true-color support. Recommended: iTerm2,
  WezTerm, Ghostty, Kitty, Alacritty, GNOME Terminal, **Windows Terminal**.
  The legacy Windows console (`cmd.exe` window) renders the box and braille characters poorly.
- A monospace font with box-drawing and braille glyphs (JetBrains Mono, Fira Code,
  Cascadia Code, Menlo, DejaVu Sans Mono all work).
- Terminal size of at least 120×36 for the full layouts (160×44 is ideal).
- For remote machines: OpenSSH (`ssh`) on your machine and key-based SSH access.

Argus needs no account, server, port or network connection to watch the local machine.

## 1. Install

### With a package manager (recommended)

The easiest way: one command installs Argus, puts it on your `PATH`, and later updates it.

**macOS and Linux — Homebrew**

```sh
brew tap serkankoyun/argus https://github.com/serkankoyun/argus
brew install serkankoyun/argus/argus
argus --version
```

Homebrew picks the right build for your machine (Apple Silicon / Intel, ARM / x86-64) and
does not trigger the macOS "unverified developer" warning. Update with `brew upgrade argus`.

**Windows — Scoop**

If you do not have [Scoop](https://scoop.sh) yet, install it first in PowerShell:

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
Invoke-RestMethod -Uri https://get.scoop.sh | Invoke-Expression
```

Then:

```powershell
scoop bucket add argus https://github.com/serkankoyun/argus
scoop install argus/argus
argus --version
```

Update with `scoop update` followed by `scoop update argus` (the first command refreshes
Scoop's package lists; without it Scoop does not see new versions). Run Argus in
**Windows Terminal**.

> **Not on winget yet.** `winget install argus` currently finds *other* programs that are
> also called Argus — do not install those. This Argus will be published on winget as
> `SerkanKoyun.Argus` (`winget install --id SerkanKoyun.Argus`). Until then use Scoop or
> the manual steps below. Check what you installed with `argus --version`: it prints
> `argus 0.1.0` (or newer) and `argus --help` mentions Claude Code.

### Manual download

Download the archive for your platform from the
[releases page](https://github.com/serkankoyun/argus/releases). Each release also contains
`SHA256SUMS` so you can verify the download.

| Platform | Archive |
|---|---|
| macOS, Apple Silicon (M1–M4) | `argus-<version>-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `argus-<version>-x86_64-apple-darwin.tar.gz` |
| Linux, x86-64 | `argus-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux, ARM64 | `argus-<version>-aarch64-unknown-linux-gnu.tar.gz` |
| Windows, x86-64 | `argus-<version>-x86_64-pc-windows-msvc.zip` |

Each archive contains the program (`argus` or `argus.exe`), a short `README.md`, this
guide as `INSTALL.md`, and `LICENSE`.

Not sure which Mac you have? Run `uname -m`: `arm64` means Apple Silicon, `x86_64` Intel.

### macOS

```sh
cd ~/Downloads
shasum -a 256 -c SHA256SUMS --ignore-missing        # optional: verify the download
tar xzf argus-v0.1.0-aarch64-apple-darwin.tar.gz    # use the x86_64 archive on Intel
sudo mkdir -p /usr/local/bin
sudo mv argus-v0.1.0-aarch64-apple-darwin/argus /usr/local/bin/
sudo xattr -d com.apple.quarantine /usr/local/bin/argus
argus --version
```

The `xattr` line is needed because the binary is not signed by Apple; without it macOS
reports that the app "cannot be opened". If you prefer not to use `sudo`, put the binary in
`~/.local/bin` and add that folder to your `PATH` (see below).

### Linux

```sh
cd ~/Downloads
sha256sum -c SHA256SUMS --ignore-missing            # optional: verify the download
tar xzf argus-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
mkdir -p ~/.local/bin
mv argus-v0.1.0-x86_64-unknown-linux-gnu/argus ~/.local/bin/
argus --version
```

If `argus` is not found, add `~/.local/bin` to your `PATH`:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc   # or ~/.zshrc
source ~/.bashrc
```

### Windows

1. Download `argus-v0.1.0-x86_64-pc-windows-msvc.zip` and extract it.
2. Create a folder for command-line tools, e.g. `C:\Tools`, and move `argus.exe` there.
3. Add the folder to your `PATH`: Start → "Edit environment variables for your account" →
   *Path* → *Edit* → *New* → `C:\Tools` → OK. Open a **new** terminal afterwards.
4. Run it in **Windows Terminal** (PowerShell or Command Prompt tab):

   ```powershell
   argus --version
   ```

If Windows SmartScreen warns about an unrecognized app, choose **More info → Run anyway**
(the binary is not code-signed yet).

If Claude Code runs inside **WSL**, install the Linux build inside WSL and run Argus there,
because the session files live in the Linux home directory.

### Build from source

Requires a recent stable [Rust toolchain](https://rustup.rs).

```sh
git clone https://github.com/serkankoyun/argus
cd argus
cargo install --path .        # installs `argus` into ~/.cargo/bin
```

## 2. First run

Start with the demo to learn the screens without touching real data:

```sh
argus --demo
argus --demo --lang tr        # Turkish UI
```

Then watch your own Claude Code sessions:

```sh
argus
```

Argus shows sessions active in the last 24 hours. If the list is empty, start a Claude Code
session in another terminal; it appears within a second or two.

Essential keys:

| Key | Action |
|---|---|
| `1`–`9`, `0` or `F1`–`F10` | Jump to a view (Overview, Sequence, Agent, Fleet, Graph, Room, Lanes, Channel, Task map, Pair); you can also click the tabs |
| `t` | Teams view (coordinator, a card per busy team); `↑↓` choose a team, `Enter` open it, `esc` back to all teams |
| `T` | Filter every view to the next team (since v0.1.8; earlier `t` and `T` were swapped) |
| `s` | Filter every view to the next host; on the Fleet view `↑↓` + `Enter` picks one |
| `f` | Show / hide finished agents (hidden by default) |
| `H` | History: finished agents with duration, cost and outcome |
| `↑` `↓` | Select an agent (the selection follows you across views) |
| `Enter` | Agent detail |
| `g` | Group Graph and Room by team instead of host |
| `z` | Zoom the Lanes timeline |
| `space` | Pause / resume |
| `:` | Command bar (see below) |
| `q` | Quit |

Useful options:

```sh
argus --lang en                    # UI language (default: system locale)
argus --claude-dir /path/.claude   # another Claude config directory
argus snapshot --wait 2            # print one snapshot as JSON (for scripts)
```

## 3. Hooks: exact states and approvals from Argus (optional, recommended)

```sh
argus hook --install
```

This adds two kinds of hooks to `~/.claude/settings.json` (a backup is written to
`settings.json.bak`). Restart running Claude Code sessions so they pick them up.

- **Lifecycle events** (`argus hook --event`): Claude Code reports every tool start and
  end, every finished turn and every "waiting for your input". Without them Argus guesses
  from transcripts, which stay silent during long commands, so a long `Bash` run can look
  like a pending approval. The events are small lines in `~/.claude/argus/events/`, kept 3
  days. Other tools can write the same format: see [EVENTS.md](EVENTS.md).
- **Approvals** (`argus hook`, a `PermissionRequest` hook), described below.

When an agent needs permission, Argus shows it in the header, the Room view and the Channel
view. Press `a` to approve or `r` to reject. If nobody answers within 120 seconds, Claude
Code shows its normal prompt. Change the wait with:

```sh
argus hook --install --wait 300
```

To remove the hooks, delete the entries whose command contains `argus` … `hook` from
`~/.claude/settings.json`, or restore `settings.json.bak`.

For remote machines, run `argus hook --install` **on that machine** (the collector Argus
installed is at `~/.cache/argus/argus`: `~/.cache/argus/argus hook --install`); events and
approvals then reach your local Argus over SSH.

### Grouping agents into your own teams

Shortcut: run `argus init` on the machine that runs the agents. It reads the agent names
of the last 24 hours, finds a `role-team-work` pattern if there is one, and prints the
rules and how many agents each team would get. `argus init --write` saves them (keeping a
`.bak` of an existing file). For remote machines: `~/.cache/argus/argus init --write`.

Argus groups agents by Agent Teams team when you use that feature, by a `team` reported in
events ([EVENTS.md](EVENTS.md)), and otherwise by project folder. If your orchestrator encodes
the team in agent names (for example `qa-payments-review`), describe it in
`~/.claude/argus/teams.json` **on the machine that runs the agents**:

```json
{
  "strip": ["developer-", "tester-", "qa-"],
  "teams": ["payments", "search-api"],
  "teams_file": "/path/to/ownership/teams.json",
  "aliases": { "pay": "payments" },
  "labels": { "payments": "Payments and Billing" },
  "label_key": "displayName",
  "short_names": true,
  "roles": { "developer": "Developer", "qa": "QA" },
  "coordinator": "Coordinator",
  "tasks": { "Developer · refunds": "Refund API and its tests" },
  "tasks_file": "/path/to/argus-tasks.json",
  "independent_roles": ["qa-", "security-"],
  "independent": "Independent",
  "team_roles": { "*": ["lead", "developer", "tester"], "payments": ["developer", "qa"] },
  "default": "no-team"
}
```

The first matching `strip` prefix is removed from the name, then the longest matching team
code wins. `teams_file` (optional) adds codes from a JSON file: a list of strings, or objects
with `code`, at the top level or under `teams`. A code matches whole name segments only. `aliases` maps retired codes to current
ones, so agents from before a team rename stay with that team. Team display names come from
`labels`, or from the field named by `label_key` in `teams_file` objects. With `short_names`,
grouped agents are shown as `<role> · <work>` (`roles` gives readable role names), and
`coordinator` names the sessions that start subagents and puts them in a group of their own.
`team_roles` lists a team's usual roles (role keys; `*` for every team without its own list):
each Teams card shows them under its counts, `●` working, `◐` waiting, `○` nobody on it now,
`×2` when two agents hold the role. Without `team_roles` the card lists the roles seen in the
team in the last 24 hours, leads first.
`independent_roles` puts agents whose name starts with one of those prefixes (reviewers that work
across teams) in the `independent` group. `tasks` (or a `tasks_file` holding the same object) sets an agent's task line by its displayed
name; otherwise the task is the spawn description without a leading run id, the first
sentence of the agent's prompt, or, for a coordinator, the latest prompt you typed. A missing
`teams_file`/`tasks_file` is skipped and reported in the footer and by `argus init`, which
also keeps your existing rules and only adds to them. Agents matching no code get
`default`.
Changes are picked up within a second; no restart needed.

### Runs and stages in the Task map

If an orchestrator keeps its units of work in a `runs/` folder (next to a session's working
folder or up to three levels above it), the Task map (`9`) shows one row per run instead of
task boxes: its goal and team, eight stage chips (Analysis, Plan, Development, Test, QA,
Security, CI, Merge) and who has the current stage. A **Bottlenecks** panel lists the stages
held longest, rework loops (rejections per stage) and blocked stages.

```
runs/R-20261114-1720-nav-demo/
  run.json      {"id": "...", "goal": "...", "createdAt": "...",
                 "stages": [{"id": "dev", "agent": "orchestra-developer"}, ...]}
  dev.md        ---\nstage: dev\nstatus: done\n---   (pending | in-progress | done | blocked | skipped)
  qa.md         a line "Karar: KABUL" / "Decision: REJECT" / "RET" alone; the last one counts
  qa-2.md       extra files <stage>-<suffix>.md belong to the same stage
```

Everything is optional. Files left `pending` are templates: their text is not read as a
decision. Agents whose spawn description names the run (`<agent-name> · R-20261114-1720-x ·
<task>` or `R-20261114-1720-x <agent-name>`) fill in what files do not say: a live agent
makes its stage running (waiting when it waits), a finished one marks it done. Its stage
comes from its agent type (`stages[].agent` in `run.json`, else words such as `analyst`,
`developer`, `test`, `qa`, `security`, `ci`). Stages nobody worked on while later ones went
ahead show as skipped. Runs untouched for three days are not read; without any run the Task
map shows task dependencies as before.

## 4. Watching remote machines over SSH (optional)

Argus reuses your own `ssh`, so everything in `~/.ssh/config` works (aliases, keys,
jump hosts, ports).

1. Make sure key-based login works without a password prompt:

   ```sh
   ssh user@server true
   ```

   If it asks for a password, set up a key: `ssh-keygen -t ed25519` then
   `ssh-copy-id user@server`.

2. Start Argus with one or more hosts:

   ```sh
   argus --host user@server
   argus --host prod-1 --host gpu-2          # aliases from ~/.ssh/config
   argus --host prod-1 --no-local            # only the remote machine, not this one
   ```

   Or add a host while Argus is running: press `:` and type `connect user@server`.

   To look at one machine at a time, press `s` (cycles through hosts) or open the Fleet
   view (`4`), choose a host with `↑↓` and press `Enter`; `esc` from any view returns to
   all hosts.

   By default Argus watches **this machine and** the `--host` machines together. If this
   machine has no Claude Code sessions it is hidden; use `--no-local` to leave it out
   entirely.

3. On first connection Argus looks for `argus` on the remote machine (on its `PATH` and in
   `~/.cache/argus`, `~/.local/bin`, `~/.cargo/bin`, `/usr/local/bin` and the Homebrew
   folders). If it is missing, Argus installs it into `~/.cache/argus/argus` automatically:
   - same OS and CPU as your machine: Argus copies itself over SSH;
   - **different OS or CPU** (e.g. Windows → Linux, Mac → Linux, x86 → ARM): the server
     downloads the matching build from the GitHub releases page with `curl` or `wget`.

   So connecting from Windows to a Linux server works out of the box, as long as the
   server can reach github.com. If it cannot (offline/firewalled servers), install manually:

   ```sh
   ssh user@server
   uname -m                       # x86_64 → x86_64 archive, aarch64 → aarch64 archive
   mkdir -p ~/.cache/argus && cd /tmp
   curl -LO https://github.com/serkankoyun/argus/releases/download/v0.1.0/argus-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
   tar xzf argus-v*-unknown-linux-gnu.tar.gz
   mv argus-v*-unknown-linux-gnu/argus ~/.cache/argus/argus
   ~/.cache/argus/argus --version
   ```

   (Replace `v0.1.0` with the latest version from the releases page.) If the server is not
   ready, the Overview and Fleet views show the reason next to the host in amber.

   Sessions that are already running on the server appear as soon as the collector starts;
   there is no need to restart Claude Code (except after installing the approval hook).

Nothing listens on a network port; the remote collector runs only while you are connected.
The Fleet view (`4`) shows connection health, round-trip time and alerts per host.

**Windows as a remote machine:** enable the OpenSSH server (Settings → System → Optional
features → *OpenSSH Server*, then `Start-Service sshd`) and install `argus.exe` on its
`PATH`; automatic copying is supported only between Linux/macOS machines of the same type.

## 5. Starting and steering agents

| Command | What it does |
|---|---|
| `>` then a prompt | Starts a new headless Claude Code run (`claude -p`) in the selected agent's project, on its machine |
| `:msg <text>` | Sends a follow-up to the selected session (`claude -p --resume`) |
| `:run <dir> <prompt>` | Starts a run in a specific directory on the local machine |
| `:connect user@host` | Starts watching another machine |
| `:view graph` | Switches view by name |
| `:lang tr` | Switches the UI language |
| `a` / `r` | Approves / rejects the pending permission request |

Turkish aliases also work: `:mesaj`, `:çalıştır`, `:bağlan`, `:görünüm`, `:dil`.

Notes:
- New runs and resumed sessions show up like any other session within a few seconds.
- Subagents cannot be resumed directly; message their parent instead.
- Sending to a session that is open interactively in another terminal starts a parallel
  headless turn on it; prefer letting Argus start the agents you want to steer.

## 6. Troubleshooting

**`argus` starts a different program** — another tool with the same name is installed
(for example from winget or a system package). Check with `argus --help`; remove the other
one, or call this Argus by its full path.

**"No recent Claude Code sessions"** — Check that sessions exist:
`ls ~/.claude/projects/`. If Claude Code uses another config folder, start Argus with
`--claude-dir` or set `CLAUDE_CONFIG_DIR`. Only sessions modified in the last 24 hours are shown.

**Only one or two agents appear although many are running; "0 active" in the header** —
v0.1.4 hides finished agents, and v0.1.4–v0.1.5 judged a session finished when no `claude`
process ran in *exactly* its folder. Sessions that `cd` into a subproject (or teammates
running inside their lead's process) were hidden after 5 quiet minutes. Update to v0.1.6;
meanwhile press `f` to show them, or `H` to see what Argus considered finished. Background
subagents ("Async agent launched successfully") were also marked finished the moment they
started. v0.1.6 says when sessions are hidden instead of "No recent Claude Code sessions".

**`T` (or `H`) acts like `t` on Windows** — Some terminals send Shift+letter as the lowercase
letter plus Shift, so `T` cycled the team filter instead of opening Teams. Fixed in v0.1.6;
`F11` (Teams), `F12` (History) or a mouse click on the tab work in any version.

**Background subagents disappear while they run tests; the coordinator is renamed after a
run folder** — Before v0.1.11 a subagent that wrote nothing for 5 minutes (a long test run)
counted as finished, and a session was named after its *current* folder. Now a subagent
with a tool call in flight stays live, and sessions keep the name of the folder they
started in. Since v0.1.12 a subagent that is resumed after it stopped (a coordinator
messaging it for another round) shows as working again instead of staying finished.

**A subagent waiting on its own background command shows as finished, or one that handed
back its report shows as working** — Fixed in v0.1.17: a command started with
`run_in_background` keeps the agent working (`[bg 3m10s] cmd`), and a `SubagentHandback`
call finishes it. Until v0.1.27 an agent that ended its turn while it waited (a team lead
saying "waiting for the tester", a coordinator polling CI in the background) still turned
finished or idle after a few quiet minutes; now it shows `◐ waiting for the agents it
started: …` or `waiting for its background job: …`, and Open requests show its task as
`◐ WAITING`. Messages sent to a raw agent id are named after that agent.

**Agents started by a team lead show the coordinator as their parent, as `agent-…`, or in
the default group** — Fixed in v0.1.23: the parent is whoever's spawn result returned the
agent's id, an unnamed agent takes its name from a description shaped like
`<agent-name> · <run> · <task>`, and an agent whose name gives no team inherits its
parent's team. Team cards list each lead with its agents indented below it.

**Open requests keep finished work as "working · 31m"** — Fixed in v0.1.24: a task counts as
answered when its agent finishes (hand-back included), and targets are named, not shown as ids.

**Room and Network show raw commands (`cd /tmp/... && ...`) or `agent-…` names** — Since
v0.1.26 commands lose their `cd`/variable set-up, Room and Network boxes show each agent's task
and activity like the Teams view, and an agent named `agent-…` takes the name its task starts
with (`<agent-name> · <run> · <task>`). Spawn events that carry `agentId` (a lead starting its
own agents) set the parent.

**Task map shows boxes titled with a session id, "106 tasks, 102 done"** — Before v0.1.29 it
mapped agents. It now maps orchestrator runs when a `runs/` folder exists (see "Runs and
stages in the Task map"). Agents started with a description like `R-20261114-1842-x
analist-hekim-verisi` were named after the run id; since v0.1.29 the word after the run id
is their name.

**After picking a team, every view stays on that team** — That is the team filter (the header
shows `team <name>`). Press `esc` once in any view (except the agent detail, where `esc`
goes back) to see all teams again; before v0.1.31 `esc` first went to the Overview and only
a second `esc` cleared the filter `T` also cycles teams and ends at "all".

**An unrelated session shows as a second coordinator** — Any session that started subagents
counted as a coordinator. Since v0.1.30, when `teams.json` lists team codes, only a session
that started agents of those teams (or independent reviewers) gets the `coordinator` label;
other sessions keep their project name.

**Task lines start with `· ` or independent reviewers sit inside a team** — Event writers that
move the run id into its own field may keep its separator (`"desc":"· Kurulum sihirbazı …"`);
since v0.1.28 the separator is dropped. Reviewers that work across teams (independent QA,
security) get a group of their own with `"independent_roles": ["qa-", "guvenlik-"]` and
`"independent": "Bağımsız"` in `teams.json`; their name keeps the team they review.

**Messages stay "working · 2h" in Open requests** — Plain messages have no reply marker.
Since v0.1.6 a message counts as answered when the recipient writes back, when Agent Teams
marks it read, or when the sender sends a newer one; unanswered messages leave the list
after 15 minutes.

**A remote host is connected and its agents are working, but little or nothing updates** —
Before v0.1.4 Argus kept using whatever collector it had installed on the server the first
time (`~/.cache/argus/argus`), even after you updated Argus on your own machine, so an old
collector could miss newer Claude Code data. Update Argus locally; from v0.1.4 the server copy
is replaced automatically when its version differs. To check what the server sees, run on the
server: `~/.cache/argus/argus snapshot | head -c 2000`. If that shows no agents, the agents run
as another user or with another config folder (`CLAUDE_CONFIG_DIR`): connect as that user.
If it shows the agents but their state lags, install the hooks on the server (section 3).

**Parts of the screen disappear or shift when switching views (Windows)** — Some fonts draw
certain symbols two cells wide. Since v0.1.1 Argus avoids those symbols and repaints the whole
screen on every view change; update if you see this. Using Cascadia Mono or JetBrains Mono
in Windows Terminal also helps.

**`scoop update` says "Your local changes to the following files would be overwritten by
merge" (Cargo.toml, bucket/argus.json, docs/media/...)** — The Argus bucket is a copy of this
repository and Git on Windows changed files in it (line endings), so Scoop cannot refresh it
and keeps the old version. Re-add the bucket, then update:

```powershell
scoop bucket rm argus
scoop bucket add argus https://github.com/serkankoyun/argus
scoop update argus
```

Since v0.1.16 the repository disables line-ending conversion, so this should not come back.

**`argus --version` still shows the old version after updating** — Work through these in
order:

1. Quit every running Argus (`q`). On Windows a running program cannot be replaced, and the
   update can fail quietly.
2. Refresh the package lists, then update:
   `scoop update` then `scoop update argus` (Windows), or
   `brew update && brew upgrade argus` (macOS/Linux).
3. Check that only one copy is on your `PATH`: `where.exe argus` (Windows) or
   `which -a argus`. With Scoop the only line should end in `scoop\shims\argus.exe`;
   remove any copy you installed manually.
4. Still old? Re-add the source and update again:
   `scoop bucket rm argus`, `scoop bucket add argus https://github.com/serkankoyun/argus`,
   `scoop update argus` — or reinstall: `scoop uninstall argus`, `scoop cache rm argus`,
   `scoop install argus/argus`. (Homebrew: `brew untap serkankoyun/argus`, then tap and
   install again.)

Remote copies in `~/.cache/argus/` are only used on servers and do not affect your local
version.

**Argus crashes with "index outside of buffer" (v0.1.2 and older)** — Happened when more
agents existed than fit on screen (common with real servers). Fixed in v0.1.3; update Argus.

**Keys do nothing while remote hosts are connected (v0.1.1 and older)** — The background
`ssh` process read the keyboard and swallowed key presses, so keys worked in `--demo` but
not with `--host`. Fixed in v0.1.2; update Argus.

**Number keys do not switch views** — Use `F1`–`F10`, `Tab` / `Shift+Tab`, or click the
tabs with the mouse. Start Argus with `argus --debug-keys` to see which key it receives
(bottom right) and include that in an issue.

**Boxes, question marks or misaligned columns** — Use a font with box-drawing and braille
glyphs and a modern terminal (on Windows: Windows Terminal, not the legacy console).

**Colors look wrong or washed out** — Enable true color in your terminal. Inside tmux add
`set -g default-terminal "tmux-256color"` and `set -as terminal-features ",*:RGB"`.

**Layout is cramped** — Enlarge the window or reduce the font size; 160×44 is ideal.

**macOS: "cannot be opened because the developer cannot be verified"** — Run
`sudo xattr -d com.apple.quarantine /usr/local/bin/argus`.

**Remote host stays "connecting…" or shows an error** — The reason is shown in amber next
to the host (Overview) and in the Fleet view's alerts (`4`).
- `could not install argus on remote (…)`: the server could not download Argus (no access to
  github.com, or no `curl`/`wget`); install it manually (section 4, step 3).
- `Permission denied`, `Host key verification failed` or `ssh failed`: run
  `ssh -o BatchMode=yes user@server true` yourself; it must succeed without any prompt.
- Connected (green) but no agents: the server has no Claude Code session active in the last
  24 hours. Check with `ssh user@server ls ~/.claude/projects`.

**Costs look different from my bill** — Costs are estimates from public per-token prices;
subscriptions and discounts are not known to Argus.

**An agent shows "waiting for approval" but nothing is pending, or "working" while it
waits for you** — Without the hooks, Argus infers states from transcripts, which Claude
Code writes only when a step finishes: a long command looks like a pending approval. Install
the hooks (section 3) on every machine that runs agents. Long commands also show how long
they have been running, for example `[6m12s] npm test`. Since v0.1.9 sessions in a mode that
never asks (`auto`, `bypassPermissions`) are never shown as waiting for approval.

Still stuck? [Open an issue](https://github.com/serkankoyun/argus/issues) with your OS,
terminal, `argus --version` and, if possible, a screenshot.

## 7. Updating and uninstalling

**Update:** `brew update && brew upgrade argus`, or `scoop update` then `scoop update argus`. For a manual install, download
the new release and replace the binary the same way you installed it.
Remote copies in `~/.cache/argus/` are refreshed by deleting that folder on the remote.

**Uninstall:** `brew uninstall argus` (and `brew untap serkankoyun/argus`), or
`scoop uninstall argus` (and `scoop bucket rm argus`). For a manual install:

```sh
rm /usr/local/bin/argus          # or ~/.local/bin/argus, or ~/.cargo/bin/argus
rm -rf ~/.claude/argus           # approval requests/decisions, if you used the hook
```

Then remove the hook from `~/.claude/settings.json` if you installed it. On remote
machines also delete `~/.cache/argus/`. On Windows delete `argus.exe` and remove its folder
from `PATH` if nothing else uses it.
