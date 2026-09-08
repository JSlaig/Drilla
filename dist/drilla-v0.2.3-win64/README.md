# Drilla — SSH Tunnel CLI

An interactive terminal CLI for managing SSH tunnels on Windows (Linux support pending). It wraps the OpenSSH binary and supports multiple jump hosts via `ProxyJump` chains.

## Features

- Interactive TUI (list tunnels, create/edit/duplicate/delete, run/stop)
- Multiple jump hosts per tunnel (built into a `-J` proxy jump chain)
- Password auth per jump host (and for direct tunnels) via SSH_ASKPASS
- Local port forwarding
- Folders: optional per-tunnel folder shown as group headers in the list
- Vim-style navigation (`j`/`k`) and `/` incremental search
- Selected-tunnel details pane (jumps, command, output) on the right
- Config persisted to `~/.ssh/tunnels.json`

## Requirements

- OpenSSH client on `PATH` (`ssh -V` to verify) — ships with Windows 10/11
- Windows or Linux
- No Rust toolchain needed to run: use a prebuilt release zip

## Install (end user, no cargo needed)

1. Download `drilla-v0.2.3-win64.zip` from Releases.
2. Unzip and run `install.bat` (or just double-click `drilla.exe`).

`install.bat` copies the exe to `%LOCALAPPDATA%\Programs\drilla` and adds it to
your PATH, so you can launch it from any terminal with:

```
drilla
```

`drll` is a built-in alias that also launches it, so both of these work:

```
drilla
drll
```

To confirm which build you're running:

```
drilla --version
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
| `j` / `k` or arrows | Navigate (folder headers are skipped) |
| `/` | Search (match name, host, ports, jump hosts) |
| `n` | Create a new tunnel |
| `c` | Duplicate the selected tunnel as `"name (copy)"` |
| `e` | Edit the selected tunnel |
| `d` | Delete the selected tunnel (confirm with `y`, cancel with `n`/`Esc`) |
| `q` | Quit (saves config) |

From the create/edit form (`Tab` cycles fields):

| Key | Action |
|-----|--------|
| `Tab` | Next field: Name → Folder → Local Port → Jumps → Target Host → Target Port → Target Password → Legacy algorithms |
| `a` | Add a jump host (when on Jumps) |
| `e` | Edit the selected jump host |
| `x` | Remove the selected jump host |
| `j` / `k` | Move between jump hosts |
| `Enter` | Save |
| `Esc` | Back to list |

Jump host form fields: User → Host → Port → Password.

**Legacy algorithms** (checkbox `[x]`, toggle by pressing any key while focused) —
enable when the server only offers old host key types like `ssh-rsa` / `ssh-dss`.
It adds `-o HostKeyAlgorithms=+ssh-rsa,ssh-dss` plus legacy kex/ciphers/MACs, which
modern OpenSSH clients disable by default. You'll know you need it if the tunnel
output says `no matching host key type found. Their offer: ssh-rsa,ssh-dss`.

A legacy tunnel's command line looks just like a normal one (standard `-J` for
jumps, one password prompt per host). To make the legacy options reach the *nested*
`-J` jump processes (which ignore command-line `-o` but read `~/.ssh/config`),
drilla stages a small *managed* `Host <ip>` block in `~/.ssh/config` while a
legacy tunnel runs, listing exactly the jump + login servers that tunnel touches.
It is removed automatically once no legacy tunnel needs it, and any of your other
config lines are preserved.

Typical JSON if you edit `~/.ssh/tunnels.json` by hand:

```json
{
  "tunnels": [
    {
      "name": "Legacy box",
      "jumps": [],
      "target": { "host": "oldbox.example.com", "port": 22 },
      "local_port": 2222,
      "legacy": true
    }
  ]
}
```

## Config format

Stored at `~/.ssh/tunnels.json`:

```json
{
  "tunnels": [
    {
      "name": "Production DB",
      "folder": "prod",
      "jumps": [
        { "user": "alice", "host": "bastion1.example.com", "port": 22, "password": "s3cret" },
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

When any jump (or the target, for direct tunnels) has a `password`, drilla sets
`SSH_ASKPASS` to its own executable and answers the password prompts for each hop.

> **Security note:** passwords are stored in plaintext in `~/.ssh/tunnels.json` and
> briefly in a temp file while a tunnel runs. Prefer SSH keys when possible.