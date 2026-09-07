# SSH Tunnel CLI

An interactive terminal CLI for managing SSH tunnels on Windows (Linux support pending). It wraps the OpenSSH binary and supports multiple jump hosts via `ProxyJump` chains.

## Features

- Interactive TUI (list tunnels, create/edit/delete, run/stop)
- Multiple jump hosts per tunnel (built into a `-J` proxy jump chain)
- Local port forwarding
- Optional identity key path per tunnel
- Config persisted to `~/.ssh/tunnels.json`

## Requirements

- OpenSSH client on `PATH` (`ssh -V` to verify) — ships with Windows 10/11
- Windows or Linux

## Usage

```
cargo build --release
```

Run the resulting binary at `target\release\sshtunnelcli.exe` (or `cargo run`).

### Keys

From the tunnel list:

| Key | Action |
|-----|--------|
| `Enter` | Run / stop the selected tunnel |
| `n` | Create a new tunnel |
| `e` | Edit the selected tunnel |
| `d` | Delete the selected tunnel |
| `q` | Quit (saves config) |

From the create/edit form:

| Key | Action |
|-----|--------|
| `Tab` | Next input field |
| `a` | Add a jump host |
| `e` | Edit the selected jump host |
| `x` | Remove the selected jump host |
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
      "local_port": 5432,
      "key_path": "~/.ssh/id_rsa"
    }
  ]
}
```

This generates roughly:

```
ssh -N -T -J alice@bastion1.example.com,bob@bastion2.example.com:2222 \
    -L5432:db.internal:5432 -i ~/.ssh/id_rsa alice@bastion1.example.com
```