use std::collections::HashMap;
use std::process::{Child, Command, Stdio};

use crate::models::{JumpHost, Tunnel};

pub struct SshManager {
    procs: HashMap<String, Child>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            procs: HashMap::new(),
        }
    }

    pub fn is_running(&mut self, name: &str) -> bool {
        if let Some(child) = self.procs.get_mut(name) {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.procs.remove(name);
                    false
                }
                Ok(None) => true,
                Err(_) => {
                    self.procs.remove(name);
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn running_snapshot(&mut self, names: &[&str]) -> Vec<(String, bool)> {
        names
            .iter()
            .map(|name| {
                let running = match self.procs.get_mut(*name) {
                    Some(child) => child.try_wait().map(|s| s.is_none()).unwrap_or(false),
                    None => false,
                };
                (name.to_string(), running)
            })
            .collect()
    }

    pub fn start(&mut self, tunnel: &Tunnel) -> Result<(), String> {
        if self.is_running(&tunnel.name) {
            return Err(format!("tunnel '{}' is already running", tunnel.name));
        }
        let mut cmd = build_ssh_command(tunnel);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());
        let child = cmd.spawn().map_err(|e| format!("failed to spawn ssh: {e}"))?;
        self.procs.insert(tunnel.name.clone(), child);
        Ok(())
    }

    pub fn stop(&mut self, name: &str) -> Result<(), String> {
        if let Some(mut child) = self.procs.remove(name) {
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }

    pub fn refresh_all(&mut self, names: &[String]) {
        let names_copy: Vec<String> = names.to_vec();
        for name in names_copy {
            self.is_running(&name);
        }
    }
}

pub fn build_ssh_command(tunnel: &Tunnel) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.arg("-N").arg("-T");

    if !tunnel.jumps.is_empty() {
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

    #[test]
    fn multi_jump_chain() {
        let tunnel = Tunnel {
            name: "test".into(),
            jumps: vec![
                JumpHost {
                    user: "alice".into(),
                    host: "jump1.example.com".into(),
                    port: 22,
                },
                JumpHost {
                    user: "bob".into(),
                    host: "jump2.example.com".into(),
                    port: 2222,
                },
                JumpHost {
                    user: String::new(),
                    host: "jump3.example.com".into(),
                    port: 22,
                },
            ],
            target: Target {
                host: "db.internal".into(),
                port: 5432,
            },
            local_port: 5433,
        };

        let args = cmd_to_args(&build_ssh_command(&tunnel));
        assert_eq!(
            args.clone(),
            vec![
                "ssh",
                "-N",
                "-T",
                "-J",
                "alice@jump1.example.com,bob@jump2.example.com:2222,jump3.example.com",
                "-L5433:db.internal:5432",
                "alice@jump1.example.com"
            ]
        );
    }

    #[test]
    fn no_jumps_uses_target() {
        let tunnel = Tunnel {
            name: "direct".into(),
            jumps: vec![],
            target: Target {
                host: "server.example.com".into(),
                port: 22,
            },
            local_port: 8080,
        };

        let args = cmd_to_args(&build_ssh_command(&tunnel));
        assert_eq!(args[1], "-N");
        assert_eq!(args[2], "-T");
        assert_eq!(args[3], "-L8080:server.example.com:22");
        assert_eq!(args[4], "server.example.com");
    }
}
