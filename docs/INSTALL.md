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

Update with `scoop update argus`. Run Argus in **Windows Terminal**.

`winget install argus` is planned; until then use Scoop or the manual steps below.

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
| `1`–`9`, `0` | Jump to a view (Overview, Sequence, Agent, Fleet, Graph, Room, Lanes, Channel, Task map, Pair) |
| `T` | Teams view; `←→` choose a team, `⏎` open it, `esc` back to all teams |
| `t` | Filter every view to the next team |
| `↑` `↓` | Select an agent (the selection follows you across views) |
| `⏎` | Agent detail |
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

## 3. Approving tool calls from Argus (optional)

Claude Code can ask Argus before showing its permission prompt:

```sh
argus hook --install
```

This adds a `PermissionRequest` hook to `~/.claude/settings.json` (a backup is written to
`settings.json.bak`). Restart running Claude Code sessions so they pick it up.

When an agent needs permission, Argus shows it in the header, the Room view and the Channel
view. Press `a` to approve or `r` to reject. If nobody answers within 120 seconds, Claude
Code shows its normal prompt. Change the wait with:

```sh
argus hook --install --wait 300
```

To remove the hook, delete the `PermissionRequest` entry whose command ends in `hook` from
`~/.claude/settings.json`, or restore `settings.json.bak`.

For remote machines, run `argus hook --install` **on that machine**; approvals made in
your local Argus are delivered over SSH.

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
   argus --host prod-1 --no-local            # only the remote machine
   ```

   Or add a host while Argus is running: press `:` and type `connect user@server`.

3. On first connection Argus looks for `argus` on the remote machine. If it is missing and
   the remote has the same OS and CPU type, Argus copies itself to `~/.cache/argus/argus`
   there. Otherwise install Argus on the remote machine as in step 1.

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

**"No recent Claude Code sessions"** — Check that sessions exist:
`ls ~/.claude/projects/`. If Claude Code uses another config folder, start Argus with
`--claude-dir` or set `CLAUDE_CONFIG_DIR`. Only sessions modified in the last 24 hours are shown.

**Boxes, question marks or misaligned columns** — Use a font with box-drawing and braille
glyphs and a modern terminal (on Windows: Windows Terminal, not the legacy console).

**Colors look wrong or washed out** — Enable true color in your terminal. Inside tmux add
`set -g default-terminal "tmux-256color"` and `set -as terminal-features ",*:RGB"`.

**Layout is cramped** — Enlarge the window or reduce the font size; 160×44 is ideal.

**macOS: "cannot be opened because the developer cannot be verified"** — Run
`sudo xattr -d com.apple.quarantine /usr/local/bin/argus`.

**Remote host stays "connecting…" or shows an error in the Fleet view** — Run
`ssh -o BatchMode=yes user@server true` yourself; it must succeed without prompts. If the
remote is a different OS/CPU, install Argus there manually.

**Costs look different from my bill** — Costs are estimates from public per-token prices;
subscriptions and discounts are not known to Argus.

**An agent shows "waiting for approval" but nothing is pending** — Without the hook,
Argus infers waiting from a tool call that has made no progress for 20 seconds. Install
the hook (section 3) for exact approval tracking.

Still stuck? [Open an issue](https://github.com/serkankoyun/argus/issues) with your OS,
terminal, `argus --version` and, if possible, a screenshot.

## 7. Updating and uninstalling

**Update:** `brew upgrade argus` or `scoop update argus`. For a manual install, download
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
