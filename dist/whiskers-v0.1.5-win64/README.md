# Whiskers — SSH Tunnel CLI

An interactive terminal CLI for managing SSH tunnels on Windows (Linux support pending). It wraps the OpenSSH binary and supports multiple jump hosts via `ProxyJump` chains.

## Features

- Interactive TUI (list tunnels, create/edit/delete, run/stop)
- Multiple jump hosts per tunnel (built into a `-J` proxy jump chain)
- Password auth per jump host (and for direct tunnels) via SSH_ASKPASS
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
| `Tab` | Next field: Name → Local Port → Jumps → Target Host → Target Port → Target Password → Legacy algorithms |
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

Note: for tunnels with jump hosts, OpenSSH's `-J` negotiates the middle hops from
nested `ssh -W` processes that ignore command-line `-o` options, so those hops
couldn't use legacy algorithms. Whiskers therefore builds the jump chain itself as
an explicit nested `ProxyCommand` instead, so every hop is negotiated by a local
ssh carrying the tunnel's options — no changes to `~/.ssh/config` are needed.

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

When any jump (or the target, for direct tunnels) has a `password`, whiskers sets
`SSH_ASKPASS` to its own executable and answers the password prompts for each hop.

> **Security note:** passwords are stored in plaintext in `~/.ssh/tunnels.json` and
> briefly in a temp file while a tunnel runs. Prefer SSH keys when possible.