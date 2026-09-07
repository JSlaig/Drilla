use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
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
    // name -> hosts that need legacy algorithms while this tunnel runs
    legacy_hosts: HashMap<String, Vec<String>>,
    // override for the managed ~/.ssh/config path (tests inject a temp path)
    config_path: Option<PathBuf>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            procs: HashMap::new(),
            outputs: HashMap::new(),
            legacy_hosts: HashMap::new(),
            config_path: None,
        }
    }

    #[cfg(test)]
    fn with_config_path(path: PathBuf) -> Self {
        let mut m = Self::new();
        m.config_path = Some(path);
        m
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
                    if self.legacy_hosts.remove(&n).is_some() {
                        let _ = sync_legacy_config_at(self.config_path.clone(), &self.active_legacy_tunnels());
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
        cmd            .env("SSH_ASKPASS", askpass_program())
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DRILLA_ASKPASS_MODE", "1")
            .env("DRILLA_ASKPASS_FILE", &path);

        // For legacy tunnels, the nested `-J` hops and the login host must see
        // the legacy host-key/algo options. They ignore command-line -o, but
        // read ~/.ssh/config — so stage a managed Host block for this tunnel.
        if tunnel.legacy {
            let hosts = tunnel_ssh_hosts(tunnel);
            self.legacy_hosts.insert(tunnel.name.clone(), hosts.clone());
            // Build the block from the staged hosts directly — the tunnel is
            // added to `self.procs` only after spawn, so active_legacy_tunnels()
            // would omit it here.
            let mut active: Vec<(String, Vec<String>)> = Vec::new();
            active.push((tunnel.name.clone(), hosts));
            for (n, h) in self.legacy_hosts.iter() {
                if n != &tunnel.name {
                    active.push((n.clone(), h.clone()));
                }
            }
            let _ = sync_legacy_config_at(self.config_path.clone(), &active);
        }

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
        if self.legacy_hosts.remove(name).is_some() {
            let _ = sync_legacy_config_at(self.config_path.clone(), &self.active_legacy_tunnels());
        }
        Ok(())
    }

    pub fn refresh_all(&mut self, names: &[String]) {
        self.refresh();
        let _ = names;
    }

    /// The set of tunnels currently known to need legacy algorithms, so the
    /// managed ~/.ssh/config block can be rebuilt from the active ones.
    fn active_legacy_tunnels(&self) -> Vec<(String, Vec<String>)> {
        self.legacy_hosts
            .iter()
            .filter(|(n, _)| self.procs.contains_key(*n))
            .map(|(n, h)| (n.clone(), h.clone()))
            .collect()
    }

    /// Remove any managed legacy block that is no longer needed (e.g. on app
    /// exit after all tunnels stopped).
    pub fn clear_legacy_block(&mut self) {
        self.legacy_hosts.clear();
        let _ = sync_legacy_config_at(self.config_path.clone(), &[]);
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
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("drilla"))
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
        "drilla_askpass_{}_{}.json",
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
    let Some(file) = std::env::var("DRILLA_ASKPASS_FILE").ok() else {
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

// ---------------------------------------------------------------------------
// Managed legacy-algorithms section in the user's ssh config.
//
// OpenSSH disables ssh-rsa / ssh-dss host keys by default, and the *nested*
// ssh processes that OpenSSH's `-J` uses to reach each jump host read the
// user's ssh config file -- command-line `-o` options don't reach them. So for
// legacy tunnels we maintain a managed Host block in ~/.ssh/config (for exactly
// the jump + login hosts of the currently-running legacy tunnels), written
// before the tunnel starts and removed once no legacy tunnel needs it.
// ---------------------------------------------------------------------------

const LEGACY_BEGIN: &str = "# === drilla: legacy ssh algorithms (managed) ===";
const LEGACY_END: &str = "# === end drilla legacy ===";

const LEGACY_HOST_LINES: &str = "  HostKeyAlgorithms +ssh-rsa,ssh-dss\n  PubkeyAcceptedAlgorithms +ssh-rsa\n  KexAlgorithms +diffie-hellman-group1-sha1,diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1\n  Ciphers +3des-cbc,aes128-cbc,aes192-cbc,aes256-cbc\n  MACs +hmac-sha1,hmac-md5\n  StrictHostKeyChecking no\n  UserKnownHostsFile NUL\n";

pub fn default_config_path() -> PathBuf {
    if let Ok(home) = std::env::var("USERPROFILE") {
        PathBuf::from(home).join(".ssh").join("config")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".ssh").join("config")
    } else {
        PathBuf::from("config")
    }
}

/// Hosts that are reached over SSH for this tunnel (jump hosts + login host).
fn tunnel_ssh_hosts(tunnel: &Tunnel) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for j in &tunnel.jumps {
        if !hosts.contains(&j.host) {
            hosts.push(j.host.clone());
        }
    }
    let login_host = if tunnel.jumps.is_empty() {
        tunnel.target.host.clone()
    } else {
        tunnel.jumps[0].host.clone()
    };
    if !hosts.contains(&login_host) {
        hosts.push(login_host);
    }
    hosts
}

/// Build the managed section text from the active legacy tunnels
/// (`(name, hosts)` pairs), deduping hosts across tunnels.
fn legacy_section_for(tunnels: &[(String, Vec<String>)]) -> String {
    let mut hosts: Vec<String> = Vec::new();
    for (_name, hs) in tunnels {
        for h in hs {
            if !hosts.contains(h) {
                hosts.push(h.clone());
            }
        }
    }
    if hosts.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    s.push_str(LEGACY_BEGIN);
    s.push('\n');
    for h in hosts {
        s.push_str(&format!("Host {h}\n"));
        s.push_str(LEGACY_HOST_LINES);
    }
    s.push_str(LEGACY_END);
    s.push('\n');
    s
}

fn strip_managed_section(content: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for raw in content.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim_end();
        if line == LEGACY_BEGIN {
            skipping = true;
            continue;
        }
        if line == LEGACY_END {
            skipping = false;
            continue;
        }
        if !skipping {
            out.push_str(raw);
            out.push('\n');
        }
    }
    out
}

fn write_managed_config<P: AsRef<Path>>(path: P, section: &str) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let existed = path.exists();
    let old = fs::read_to_string(path).unwrap_or_default();
    let stripped = strip_managed_section(&old);

    let mut new_content = stripped;
    if !section.is_empty() {
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(section);
    } else if new_content.trim().is_empty() {
        if existed {
            let _ = fs::remove_file(path);
        }
        return Ok(());
    }

    if new_content != old {
        if existed {
            grant_config_permissions(path)?;
        }
        fs::write(path, new_content).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        if !existed {
            grant_config_permissions(path)?;
        }
    }
    Ok(())
}

/// Windows OpenSSH refuses config files that aren't owned by the user with
/// only the owner granted access. Apply that using icacls (always present).
fn grant_config_permissions(path: &Path) -> Result<(), String> {
    let user = if let (Ok(d), Ok(u)) = (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        format!("{d}\\{u}")
    } else if let Ok(u) = std::env::var("USERNAME") {
        u
    } else {
        return Err("cannot determine current user".to_string());
    };
    let run = |args: &[&str]| -> Result<(), String> {
        let out = Command::new("icacls")
            .args(args)
            .output()
            .map_err(|e| format!("icacls: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "icacls {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(())
    };
    run(&[path.to_str().unwrap_or(""), "/inheritance:r"])?;
    run(&[path.to_str().unwrap_or(""), "/grant:r", &format!("{user}:(F)")])?;
    Ok(())
}

/// (Re)write the managed legacy section from the active legacy tunnels.
fn sync_legacy_config_at(
    path: Option<PathBuf>,
    tunnels: &[(String, Vec<String>)],
) -> Result<(), String> {
    let resolved = path.unwrap_or_else(default_config_path);
    write_managed_config(resolved, &legacy_section_for(tunnels))
}

fn base_options(tunnel: &Tunnel) -> Vec<String> {
    let mut opts: Vec<String> = Vec::new();
    if tunnel.legacy {
        // Old servers only offer ssh-rsa / ssh-dss host keys; modern OpenSSH
        // disables these by default, so re-enable them (plus the kex/ciphers
        // such hardware-era servers need), and auto-accept host keys: legacy
        // servers and internal IPs often have keys that change or were rotated,
        // which otherwise blocks the tunnel with "REMOTE HOST IDENTIFICATION
        // HAS CHANGED". A per-tunnel throwaway known_hosts file keeps drilla
        // from ever touching the user's real ~/.ssh/known_hosts.
        opts.push("StrictHostKeyChecking=no".to_string());
        opts.push("UserKnownHostsFile=NUL".to_string());
        opts.push("HostKeyAlgorithms=+ssh-rsa,ssh-dss".to_string());
        opts.push("PubkeyAcceptedAlgorithms=+ssh-rsa".to_string());
        opts.push(
            "KexAlgorithms=+diffie-hellman-group1-sha1,diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1"
                .to_string(),
        );
        opts.push("Ciphers=+3des-cbc,aes128-cbc,aes192-cbc,aes256-cbc".to_string());
        opts.push("MACs=+hmac-sha1,hmac-md5".to_string());
    } else {
        opts.push("StrictHostKeyChecking=accept-new".to_string());
    }
    opts.push("ConnectTimeout=15".to_string());
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

fn jump_string(jump: &JumpHost) -> String {
    if jump.port == 0 || jump.port == 22 {
        if jump.user.is_empty() {
            jump.host.clone()
        } else {
            format!("{}@{}", jump.user, jump.host)
        }
    } else {
        format!("{}@{}:{}", jump.user, jump.host, jump.port)
    }
}

pub fn build_ssh_command(tunnel: &Tunnel) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.arg("-N").arg("-T");
    let opts = base_options(tunnel);
    for o in option_args(&opts) {
        cmd.arg(o);
    }

    if !tunnel.jumps.is_empty() {
        // Standard, well-tested `-J` for every tunnel. For legacy tunnels the
        // legacy host-key/algo options are carried into the nested `-J` hops via
        // the managed ~/.ssh/config block (prepare_legacy_config); nested ssh
        // processes read the config file, not command-line -o options.
        let jumps: Vec<String> = tunnel
            .jumps
            .iter()
            .map(jump_string)
            .collect();
        cmd.arg("-J").arg(jumps.join(","));
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
        assert_eq!(
            args,
            vec![
                "ssh",
                "-N",
                "-T",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "-o",
                "ConnectTimeout=15",
                "-J",
                "alice@jump1.example.com,bob@jump2.example.com:2222,jump3.example.com",
                "-L5433:db.internal:5432",
                "alice@jump1.example.com"
            ]
        );
    }

    #[test]
    fn legacy_jump_uses_j_plus_managed_config() {
        let tunnel = Tunnel {
            name: "legacy".into(),
            jumps: vec![
                jump("alice", "jump1.example.com", 22, None),
                jump("bob", "jump2.example.com", 2222, None),
            ],
            target: Target {
                host: "db.internal".into(),
                port: 5432,
                password: None,
            },
            local_port: 5433,
            legacy: true,
        };

        let args = cmd_to_args(&build_ssh_command(&tunnel));
        // Like non-legacy, a legacy tunnel with jumps uses standard -J...
        assert!(args.contains(&"-J".to_string()));
        assert!(args.contains(&"alice@jump1.example.com,bob@jump2.example.com:2222".to_string()));
        // ...keeps the legacy -o options on the parent command too...
        assert!(args.contains(&"HostKeyAlgorithms=+ssh-rsa,ssh-dss".to_string()));
        // ...and auto-accepts host keys so changed keys never block it.
        assert!(args.contains(&"StrictHostKeyChecking=no".to_string()));
        assert!(args.contains(&"UserKnownHostsFile=NUL".to_string()));
        assert!(!args.contains(&"StrictHostKeyChecking=accept-new".to_string()));

        // The nested -J hops read the managed config block, which must cover
        // every hop host (jump1, jump2) plus the login host (jump1).
        let hosts = tunnel_ssh_hosts(&tunnel);
        assert!(hosts.contains(&"jump1.example.com".to_string()));
        assert!(hosts.contains(&"jump2.example.com".to_string()));
    }

    #[test]
    fn managed_section_lists_union_of_active_legacy_hosts() {
        let dir = std::env::temp_dir().join("ssht_cli_legacy_cfg");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ssh_config");

        // tunnel a touches a one-host chain; tunnel b a different one.
        let active = vec![
            (
                "a".to_string(),
                vec!["hopA1".to_string(), "hopA1".to_string()],
            ),
            ("b".to_string(), vec!["hopB".to_string()]),
        ];
        sync_legacy_config_at(Some(path.clone()), &active).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Host hopA1"), "{content}");
        assert!(content.contains("Host hopB"), "{content}");
        assert_eq!(content.matches("Host hopA1").count(), 1, "{content}");
        assert!(content.contains("HostKeyAlgorithms +ssh-rsa,ssh-dss"), "{content}");
        assert!(content.contains("StrictHostKeyChecking no"), "{content}");
        assert!(content.contains("UserKnownHostsFile NUL"), "{content}");

        // Clearing removes both entries but preserves user content.
        fs::write(&path, "# user stuff\nbefore\n").unwrap();
        sync_legacy_config_at(Some(path.clone()), &[]).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# user stuff"), "{content}");
        assert!(!content.contains("drilla: legacy"), "{content}");
    }

    #[test]
    fn start_writes_legacy_block_stop_clears_it() {        let dir = std::env::temp_dir().join("ssht_cli_start_stop");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ssh_config");

        let mut manager = SshManager::with_config_path(path.clone());

        let tunnel = Tunnel {
            name: "legacy1".into(),
            jumps: vec![
                jump("nor", "172.30.110.4", 22, None),
                jump("nor", "10.200.10.140", 22, None),
            ],
            target: Target {
                host: "192.168.4.93".into(),
                port: 8443,
                password: None,
            },
            local_port: 8443,
            legacy: true,
        };

        // start() stages the block for this tunnel before/regardless of the
        // (unreachable) connection outcome.
        let _ = manager.start(&tunnel);
        let content = fs::read_to_string(&path).unwrap_or_default();
        assert!(content.contains("Host 172.30.110.4"), "{content}");
        assert!(content.contains("Host 10.200.10.140"), "{content}");
        assert!(content.contains("HostKeyAlgorithms +ssh-rsa,ssh-dss"), "{content}");
        assert!(content.contains("StrictHostKeyChecking no"), "{content}");
        assert!(content.contains("UserKnownHostsFile NUL"), "{content}");

        // The block is present while the tunnel is "running".
        manager.stop("legacy1").ok();
        let content = fs::read_to_string(&path).unwrap_or_default();
        assert!(!content.contains("Host 172.30.110.4"), "should clear after stop: {content}");
        assert!(!content.contains("drilla: legacy"), "{content}");
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