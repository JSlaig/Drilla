# Whiskers — SSH Tunnel CLI

An interactive terminal CLI for managing SSH tunnels on Windows (Linux support pending). It wraps the OpenSSH binary and supports multiple jump hosts via `ProxyJump` chains.

## Features

- Interactive TUI (list tunnels, create/edit/delete, run/stop)
- Multiple jump hosts per tunnel (built into a `-J` proxy jump chain)
- Local port forwarding
- Vim-style navigation (`j`/`k`) and `/` incremental search
- Config persisted to `~/.ssh/tunnels.json`

## Requirements

- OpenSSH client on `PATH` (`ssh -V` to verify) — ships with Windows 10/11
- Windows or Linux
- No Rust toolchain needed to run: use a prebuilt release zip

## Install (end user, no cargo needed)

1. Download `whiskers-v0.1.0-win64.zip` from Releases.
2. Unzip and run `install.bat` (or just double-click `whiskers.exe`).

`install.bat` copies the exe to `%LOCALAPPDATA%\Programs\whiskers` and adds it to
your PATH, so you can launch it from any terminal with:

```
whiskers
```

## Build from source

```
cargo build --release
```

## Keys

From the tunnel list:

| Key | Action |
|-----|--------|
| `Enter` | Run / stop the selected tunnel |
| `j` / `k` or arrows | Navigate |
| `/` | Search (match name, host, ports, jump hosts) |
| `n` | Create a new tunnel |
| `e` | Edit the selected tunnel |
| `d` | Delete the selected tunnel |
| `q` | Quit (saves config) |

From the create/edit form (`Tab` cycles fields):

| Key | Action |
|-----|--------|
| `Tab` | Next field: Name → Local Port → Jumps → Target Host → Target Port |
| `a` | Add a jump host (when on Jumps) |
| `e` | Edit the selected jump host |
| `x` | Remove the selected jump host |
| `j` / `k` | Move between jump hosts |
| `Enter` | Save |
| `Esc` | Back to list |

## Config format

Stored at `~/.ssh/tunnels.json`:

```json
{
  "tunnels": [
    {
      "name": "Production DB",
      "jumps": [
        { "user": "alice", "host": "bastion1.example.com", "port": 22 },
        { "user": "bob", "host": "bastion2.example.com", "port": 2222 }
      ],
      "target": { "host": "db.internal", "port": 5432 },
      "local_port": 5432
    }
  ]
}
```

This generates roughly:

```
ssh -N -T -J alice@bastion1.example.com,bob@bastion2.example.com:2222 \
    -L5432:db.internal:5432 alice@bastion1.example.com
```