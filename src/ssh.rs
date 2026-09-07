use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::models::{JumpHost, Tunnel};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AskpassCred {
    #[serde(default)]
    user: String,
    host: String,
    password: String,
}

pub struct Running {
    child: Child,
    askpass: Option<PathBuf>,
    output: String,
}

pub struct SshManager {
    procs: HashMap<String, Running>,
    outputs: HashMap<String, String>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            procs: HashMap::new(),
            outputs: HashMap::new(),
        }
    }

    fn refresh(&mut self) {
        let names: Vec<String> = self.procs.keys().cloned().collect();
        for n in names {
            let dead = match self.procs.get_mut(&n) {
                Some(r) => match r.child.try_wait() {
                    Ok(Some(_)) | Err(_) => true,
                    Ok(None) => false,
                },
                None => false,
            };
            if dead {
                if let Some(mut r) = self.procs.remove(&n) {
                    r.drain_stderr();
                    self.outputs.insert(n.clone(), r.output.clone());
                    if let Some(p) = r.askpass {
                        let _ = fs::remove_file(p);
                    }
                }
            }
        }
    }

    pub fn is_running(&mut self, name: &str) -> bool {
        self.refresh();
        self.procs.contains_key(name)
    }

    pub fn running_snapshot(&mut self, names: &[&str]) -> Vec<(String, bool)> {
        self.refresh();
        names
            .iter()
            .map(|name| (name.to_string(), self.procs.contains_key(*name)))
            .collect()
    }

    pub fn output(&self, name: &str) -> Option<&str> {
        self.outputs.get(name).map(|s| s.as_str())
    }

    pub fn start(&mut self, tunnel: &Tunnel) -> Result<(), String> {
        if self.is_running(&tunnel.name) {
            return Err(format!("tunnel '{}' is already running", tunnel.name));
        }
        let mut cmd = build_ssh_command(tunnel);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());

        // Always arm the askpass helper: it answers configured passwords, and when
        // key auth is rejected it makes ssh fail fast (empty answer) instead of
        // hanging forever on a password prompt with no terminal.
        let creds = credentials_for(tunnel).unwrap_or_default();
        let path = write_askpass_file(&creds)?;
        cmd.env("SSH_ASKPASS", askpass_program())
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("WHISKERS_ASKPASS_MODE", "1")
            .env("WHISKERS_ASKPASS_FILE", &path);

        let child = cmd.spawn().map_err(|e| format!("failed to spawn ssh: {e}"))?;
        self.procs
            .insert(
                tunnel.name.clone(),
                Running {
                    child,
                    askpass: Some(path),
                    output: String::new(),
                },
            );
        self.outputs.insert(tunnel.name.clone(), String::new());
        Ok(())
    }

    pub fn stop(&mut self, name: &str) -> Result<(), String> {
        self.refresh();
        if let Some(mut r) = self.procs.remove(name) {
            let _ = r.child.kill();
            let _ = r.child.wait();
            r.drain_stderr();
            self.outputs.insert(name.to_string(), r.output.clone());
            if let Some(p) = r.askpass {
                let _ = fs::remove_file(p);
            }
        }
        Ok(())
    }

    pub fn refresh_all(&mut self, names: &[String]) {
        self.refresh();
        let _ = names;
    }
}

impl Running {
    fn drain_stderr(&mut self) {
        if let Some(mut stderr) = self.child.stderr.take() {
            let mut buf = Vec::new();
            if stderr.read_to_end(&mut buf).is_ok() {
                let text = String::from_utf8_lossy(&buf);
                if !text.trim().is_empty() {
                    if !self.output.is_empty() {
                        self.output.push('\n');
                    }
                    self.output.push_str(text.trim_end());
                }
            }
        }
    }
}

fn askpass_program() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("whiskers"))
}

fn credentials_for(tunnel: &Tunnel) -> Option<Vec<AskpassCred>> {
    let mut creds: Vec<AskpassCred> = Vec::new();
    for jump in &tunnel.jumps {
        if let Some(p) = &jump.password {
            if !p.is_empty() {
                creds.push(AskpassCred {
                    user: jump.user.clone(),
                    host: jump.host.clone(),
                    password: p.clone(),
                });
            }
        }
    }
    // Direct tunnel (no jumps): the final login is the target itself.
    if tunnel.jumps.is_empty() {
        if let Some(p) = &tunnel.target.password {
            if !p.is_empty() {
                creds.push(AskpassCred {
                    user: String::new(),
                    host: tunnel.target.host.clone(),
                    password: p.clone(),
                });
            }
        }
    }
    if creds.is_empty() {
        return None;
    }
    // Deduplicate by (user, host), keeping the first occurrence.
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut out: Vec<AskpassCred> = Vec::new();
    for c in creds {
        let key = (c.user.clone(), c.host.clone());
        if !seen.contains(&key) {
            seen.push(key);
            out.push(c);
        }
    }
    Some(out)
}

fn write_askpass_file(creds: &[AskpassCred]) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "whiskers_askpass_{}_{}.json",
        std::process::id(),
        stamp
    ));
    let json = serde_json::to_string(creds).map_err(|e| format!("askpass serialize: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("askpass file write: {e}"))?;
    Ok(path)
}

pub fn run_askpass() -> i32 {
    let Some(prompt) = std::env::args().nth(1) else {
        return 1;
    };
    let Some(file) = std::env::var("WHISKERS_ASKPASS_FILE").ok() else {
        return 1;
    };
    let creds: Vec<AskpassCred> = fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let answer = resolve_askpass(&prompt, &creds);
    print!("{}", answer);
    let _ = std::io::Write::flush(&mut std::io::stdout());
    0
}

fn resolve_askpass(prompt: &str, creds: &[AskpassCred]) -> String {
    let lower = prompt.to_lowercase();
    if lower.contains("yes/no") || lower.contains("continue connecting") {
        return "yes".to_string();
    }
    // Prompt looks like: "alice@bastion1.example.com's password: "
    let login = prompt.split('\'').next().unwrap_or("").trim();
    let (user, host) = match login.rsplit_once('@') {
        Some((u, h)) => (u.to_string(), h.to_string()),
        None => (String::new(), login.to_string()),
    };
    // Prefer an exact user@host match, then host-only, then single-cred fallback.
    for c in creds {
        if !user.is_empty() && !c.user.is_empty() && c.user == user && c.host == host {
            return c.password.clone();
        }
    }
    for c in creds {
        if !c.user.is_empty() && c.user == user && c.host == host {
            return c.password.clone();
        }
    }
    for c in creds {
        if c.host == host {
            return c.password.clone();
        }
    }
    if creds.len() == 1 {
        return creds[0].password.clone();
    }
    String::new()
}

fn base_options(tunnel: &Tunnel) -> Vec<String> {
    let mut opts: Vec<String> = Vec::new();
    opts.push("StrictHostKeyChecking=accept-new".to_string());
    opts.push("ConnectTimeout=15".to_string());
    if tunnel.legacy {
        // Old servers only offer ssh-rsa / ssh-dss host keys; modern OpenSSH
        // disables these by default, so re-enable them (plus the kex/ciphers
        // such hardware-era servers need) for this tunnel only.
        opts.push("HostKeyAlgorithms=+ssh-rsa,ssh-dss".to_string());
        opts.push("PubkeyAcceptedAlgorithms=+ssh-rsa".to_string());
        opts.push(
            "KexAlgorithms=+diffie-hellman-group1-sha1,diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1"
                .to_string(),
        );
        opts.push("Ciphers=+3des-cbc,aes128-cbc,aes192-cbc,aes256-cbc".to_string());
        opts.push("MACs=+hmac-sha1,hmac-md5".to_string());
    }
    opts
}

fn option_args(opts: &[String]) -> Vec<String> {
    let mut args = Vec::new();
    for o in opts {
        args.push("-o".to_string());
        args.push(o.clone());
    }
    args
}

/// `user@host` (or bare host) for a jump, ports used via `-p` on the command
/// line because Windows OpenSSH mishandles the `user@host:port` form inside
/// ProxyCommand strings.
fn jump_target_arg(jump: &JumpHost) -> String {
    if jump.user.is_empty() {
        jump.host.clone()
    } else {
        format!("{}@{}", jump.user, jump.host)
    }
}

/// Build a nested `ssh -W` chain string that replaces `-J`.
///
/// OpenSSH's `-J` negotiates each jump from a nested `ssh -W` process, and
/// those nested processes ignore command-line `-o` options (only the ssh
/// config file reaches them), so legacy `HostKeyAlgorithms` etc. never applied
/// to the middle hops. Here we build the chain ourselves so every hop is
/// negotiated by a local ssh with our options inline.
fn proxy_chain(tunnel: &Tunnel) -> Option<String> {
    if tunnel.jumps.is_empty() {
        return None;
    }
    let opts = base_options(tunnel);
    let prefix = {
        let mut s = String::from("ssh");
        for o in &opts {
            s.push_str(" -o ");
            s.push_str(o);
        }
        s
    };
    let mut chain = format!("{} -W %h:%p {}", prefix, jump_target_arg(&tunnel.jumps[0]));
    for j in tunnel.jumps.iter().skip(1) {
        let mut target = jump_target_arg(j);
        if j.port != 0 && j.port != 22 {
            target = format!("{} -p {}", target, j.port);
        }
        chain = format!("{} -o ProxyCommand=\"{}\" -W %h:%p {}", prefix, chain, target);
    }
    Some(chain)
}

pub fn build_ssh_command(tunnel: &Tunnel) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.arg("-N").arg("-T");
    let opts = base_options(tunnel);
    for o in option_args(&opts) {
        cmd.arg(o);
    }

    if let Some(chain) = proxy_chain(tunnel) {
        cmd.arg("-o").arg(format!("ProxyCommand={chain}"));
    }

    let target_port = if tunnel.target.port == 0 {
        22
    } else {
        tunnel.target.port
    };
    let forward = format!(
        "-L{local}:{target}:{port}",
        local = tunnel.local_port,
        target = tunnel.target.host,
        port = target_port
    );
    cmd.arg(forward);

    let target_arg = build_login(tunnel.jumps.first(), &tunnel.target.host);
    cmd.arg(target_arg);

    cmd
}

/// Render the exact ssh command that will be spawned, for display/debugging.
pub fn format_command(tunnel: &Tunnel) -> String {
    let cmd = build_ssh_command(tunnel);
    let mut parts: Vec<String> = Vec::new();
    parts.push(cmd.get_program().to_string_lossy().to_string());
    for a in cmd.get_args() {
        let a = a.to_string_lossy().to_string();
        if a.contains(' ') {
            parts.push(format!("\"{}\"", a.replace('"', "\\\"")));
        } else {
            parts.push(a);
        }
    }
    parts.join(" ")
}

fn build_login(jump: Option<&JumpHost>, fallback_host: &str) -> String {
    if let Some(j) = jump {
        if j.user.is_empty() {
            j.host.clone()
        } else {
            format!("{}@{}", j.user, j.host)
        }
    } else {
        fallback_host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Target;

    fn cmd_to_args(cmd: &Command) -> Vec<String> {
        let mut args = vec![cmd.get_program().to_str().unwrap().to_string()];
        args.extend(cmd.get_args().map(|a| a.to_str().unwrap().to_string()));
        args
    }

    fn jump(user: &str, host: &str, port: u16, password: Option<&str>) -> JumpHost {
        JumpHost {
            user: user.into(),
            host: host.into(),
            port,
            password: password.map(str::to_string),
        }
    }

    #[test]
    fn multi_jump_chain() {
        let tunnel = Tunnel {
            name: "test".into(),
            jumps: vec![
                jump("alice", "jump1.example.com", 22, None),
                jump("bob", "jump2.example.com", 2222, None),
                jump("", "jump3.example.com", 22, None),
            ],
            target: Target {
                host: "db.internal".into(),
                port: 5432,
                password: None,
            },
            local_port: 5433,
        legacy: false,
        };

        let args = cmd_to_args(&build_ssh_command(&tunnel));
        let prefix = "ssh -o StrictHostKeyChecking=accept-new -o ConnectTimeout=15";
        let inner = format!("{prefix} -W %h:%p alice@jump1.example.com");
        let level2 = format!(
            "{prefix} -o ProxyCommand=\"{inner}\" -W %h:%p bob@jump2.example.com -p 2222"
        );
        let level3 =
            format!("{prefix} -o ProxyCommand=\"{level2}\" -W %h:%p jump3.example.com");
        let expected = vec![
            "ssh".to_string(),
            "-N".to_string(),
            "-T".to_string(),
            "-o".to_string(),
            "StrictHostKeyChecking=accept-new".to_string(),
            "-o".to_string(),
            "ConnectTimeout=15".to_string(),
            "-o".to_string(),
            format!("ProxyCommand={level3}"),
            "-L5433:db.internal:5432".to_string(),
            "alice@jump1.example.com".to_string(),
        ];
        assert_eq!(args, expected);
    }

    #[test]
    fn no_jumps_uses_target() {
        let tunnel = Tunnel {
            name: "direct".into(),
            jumps: vec![],
            target: Target {
                host: "server.example.com".into(),
                port: 22,
                password: None,
            },
            local_port: 8080,
        legacy: false,
        };

        let args = cmd_to_args(&build_ssh_command(&tunnel));
        assert!(args.contains(&"-L8080:server.example.com:22".to_string()));
        assert!(args.ends_with(&["server.example.com".to_string()]));
    }

    #[test]
    fn credentials_include_jump_passwords() {
        let tunnel = Tunnel {
            name: "pw".into(),
            jumps: vec![
                jump("alice", "j1.example.com", 22, Some("s3cret")),
                jump("bob", "j2.example.com", 22, Some("hunter2")),
                jump("carol", "j3.example.com", 22, None),
            ],
            target: Target {
                host: "db.internal".into(),
                port: 5432,
                password: None,
            },
            local_port: 5432,
        legacy: false,
        };
        let creds = credentials_for(&tunnel).unwrap();
        assert_eq!(creds.len(), 2);
        assert_eq!(creds[0].host, "j1.example.com");
        assert_eq!(creds[0].password, "s3cret");
        assert_eq!(creds[1].host, "j2.example.com");
        assert_eq!(creds[1].password, "hunter2");
    }

    #[test]
    fn direct_tunnel_uses_target_password() {
        let tunnel = Tunnel {
            name: "direct".into(),
            jumps: vec![],
            target: Target {
                host: "server.example.com".into(),
                port: 22,
                password: Some("pass123".into()),
            },
            local_port: 8080,
        legacy: false,
        };
        let creds = credentials_for(&tunnel).unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].host, "server.example.com");
        assert_eq!(creds[0].password, "pass123");
    }

    #[test]
    fn legacy_tunnel_enables_ssh_rsa_and_ssh_dss() {
        let tunnel = Tunnel {
            name: "legacy".into(),
            jumps: vec![],
            target: Target {
                host: "oldbox.example.com".into(),
                port: 22,
                password: None,
            },
            local_port: 9999,
            legacy: true,
        };
        let args = cmd_to_args(&build_ssh_command(&tunnel));
        assert!(args.contains(&"-o".to_string()));
        assert!(args.contains(&"HostKeyAlgorithms=+ssh-rsa,ssh-dss".to_string()));
        assert!(args.contains(&"PubkeyAcceptedAlgorithms=+ssh-rsa".to_string()));
        assert!(args.contains(&"KexAlgorithms=+diffie-hellman-group1-sha1,diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1".to_string()));
        assert!(args.contains(&"-L9999:oldbox.example.com:22".to_string()));
    }

    #[test]
    fn legacy_toggle_off_has_no_legacy_options() {
        let tunnel = Tunnel {
            name: "modern".into(),
            jumps: vec![],
            target: Target {
                host: "server.example.com".into(),
                port: 22,
                password: None,
            },
            local_port: 8080,
            legacy: false,
        };
        let args = cmd_to_args(&build_ssh_command(&tunnel));
        assert!(!args.contains(&"HostKeyAlgorithms=+ssh-rsa,ssh-dss".to_string()));
        assert!(!args.contains(&"Ciphers=+3des-cbc,aes128-cbc,aes192-cbc,aes256-cbc".to_string()));
    }

    #[test]
    fn no_credentials_when_no_passwords() {
        let tunnel = Tunnel {
            name: "direct".into(),
            jumps: vec![],
            target: Target {
                host: "server.example.com".into(),
                port: 22,
                password: None,
            },
            local_port: 8080,
        legacy: false,
        };
        assert!(credentials_for(&tunnel).is_none());
    }

    #[test]
    fn askpass_matches_by_host() {
        let creds = vec![
            AskpassCred {
                user: "alice".into(),
                host: "j1.example.com".into(),
                password: "alpha".into(),
            },
            AskpassCred {
                user: "bob".into(),
                host: "j2.example.com".into(),
                password: "beta".into(),
            },
        ];
        assert_eq!(
            resolve_askpass("alice@j1.example.com's password: ", &creds),
            "alpha"
        );
        assert_eq!(
            resolve_askpass("bob@j2.example.com's password: ", &creds),
            "beta"
        );
        // Unknown host falls back to the single... actually two creds => empty.
        assert_eq!(resolve_askpass("carol@j9.example.com's password: ", &creds), "");
        // Host-key confirmation is answered yes.
        assert_eq!(
            resolve_askpass("Are you sure you want to continue connecting (yes/no)?", &creds),
            "yes"
        );
    }

    #[test]
    fn askpass_single_cred_fallback() {
        let creds = vec![AskpassCred {
            user: "".into(),
            host: "j1.example.com".into(),
            password: "only".into(),
        }];
        // When a jump has no username, ssh may prompt with the local user instead.
        assert_eq!(
            resolve_askpass("someuser@j1.example.com's password: ", &creds),
            "only"
        );
    }
}