# Argus

Watch and steer your Claude Code agents live, from the terminal, on any machine.

This package contains:

| File | What it is |
|---|---|
| `argus` / `argus.exe` | The program — a single file, no installer needed |
| `INSTALL.md` | Full guide: setup per platform, approvals, remote machines, troubleshooting |
| `LICENSE` | MIT license |

## Quick start

Argus is a terminal program: run it from a terminal, not by double-clicking.

Tip: a package manager installs and updates Argus with one command instead —
`brew install serkankoyun/argus/argus` (macOS/Linux, after
`brew tap serkankoyun/argus https://github.com/serkankoyun/argus`) or
`scoop install argus/argus` (Windows, after
`scoop bucket add argus https://github.com/serkankoyun/argus`). See `INSTALL.md`.
Note: `winget install argus` finds unrelated programs with the same name; this Argus is
not on winget yet.

**macOS**

```sh
sudo mv argus /usr/local/bin/
sudo xattr -d com.apple.quarantine /usr/local/bin/argus   # the binary is not signed by Apple
argus --demo
```

**Linux**

```sh
mkdir -p ~/.local/bin && mv argus ~/.local/bin/
argus --demo
```

**Windows** — use Windows Terminal (the old console window shows the screens poorly):

```powershell
.\argus.exe --demo
```

To run `argus` from any folder, move `argus.exe` to a folder on your `PATH`
(see `INSTALL.md`, section "Windows").

## First steps

```sh
argus --demo             # explore all screens with sample data
argus --demo --lang tr   # Turkish interface
argus                    # watch your own Claude Code sessions
argus --help             # all options
```

Inside Argus: `1`–`0` (or `F1`–`F10`, or a mouse click on the tabs) switch screens, `T` teams, `↑↓` select an agent, `Enter` details,
`:` command bar, `space` pause, `q` quit.

Optional:

```sh
argus hook --install           # approve Claude Code permission requests from Argus (a / r)
argus --host user@server       # also watch a remote machine over SSH (any OS combination)
argus --host user@server --no-local   # only the remote machine
```

## More

- Full guide: `INSTALL.md` in this package
- Project, screenshots and updates: https://github.com/serkankoyun/argus
- Problems or ideas: https://github.com/serkankoyun/argus/issues
