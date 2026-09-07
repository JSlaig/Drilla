use crossterm::event::KeyCode;

use crate::config::ConfigStore;
use crate::models::{Config, JumpHost, Target, Tunnel};
use crate::ssh::SshManager;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    List,
    Create,
    Edit,
    JumpAdd,
    JumpEdit,
}

pub struct ListState {
    pub selected: usize,
}

impl ListState {
    pub fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputField {
    Name,
    LocalPort,
    Jumps,
    TargetHost,
    TargetPort,
    TargetPassword,
    Legacy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JumpField {
    User,
    Host,
    Port,
    Password,
}

pub struct FormState {
    pub name: String,
    pub local_port: String,
    pub target_host: String,
    pub target_port: String,
    pub target_password: String,
    pub jumps: Vec<JumpHost>,
    pub selected_jump: usize,
    pub input: InputField,
    pub error: Option<String>,
    pub is_edit: bool,
    pub edit_index: usize,
    pub legacy: bool,
}

impl FormState {
    pub fn new_empty() -> Self {
        Self {
            name: String::new(),
            local_port: String::new(),
            target_host: String::new(),
            target_port: String::new(),
            target_password: String::new(),
            jumps: Vec::new(),
            selected_jump: 0,
            input: InputField::Name,
            error: None,
            is_edit: false,
            edit_index: 0,
            legacy: false,
        }
    }

    pub fn from_tunnel(tunnel: &Tunnel, index: usize) -> Self {
        Self {
            name: tunnel.name.clone(),
            local_port: tunnel.local_port.to_string(),
            target_host: tunnel.target.host.clone(),
            target_port: tunnel.target.port.to_string(),
            target_password: tunnel.target.password.clone().unwrap_or_default(),
            jumps: tunnel.jumps.clone(),
            selected_jump: 0,
            input: InputField::Name,
            error: None,
            is_edit: true,
            edit_index: index,
            legacy: tunnel.legacy,
        }
    }

    pub fn to_tunnel(&self) -> Result<Tunnel, String> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("Name is required".to_string());
        }
        let local_port = parse_port(&self.local_port)?;
        let target_host = self.target_host.trim().to_string();
        if target_host.is_empty() {
            return Err("Target host is required".to_string());
        }
        let target_port = if self.target_port.trim().is_empty() {
            0
        } else {
            parse_port(&self.target_port)?
        };
        let target_password = {
            let p = self.target_password.trim().to_string();
            if p.is_empty() {
                None
            } else {
                Some(p)
            }
        };
        Ok(Tunnel {
            name,
            jumps: self.jumps.clone(),
            target: Target {
                host: target_host,
                port: target_port,
                password: target_password,
            },
            local_port,
            legacy: self.legacy,
        })
    }

    pub fn cycle_input(&mut self) {
        self.input = match self.input {
            InputField::Name => InputField::LocalPort,
            InputField::LocalPort => InputField::Jumps,
            InputField::Jumps => InputField::TargetHost,
            InputField::TargetHost => InputField::TargetPort,
            InputField::TargetPort => InputField::TargetPassword,
            InputField::TargetPassword => InputField::Legacy,
            InputField::Legacy => InputField::Name,
        };
    }
}

pub struct JumpFormState {
    pub user: String,
    pub host: String,
    pub port: String,
    pub password: String,
    pub input: JumpField,
    pub error: Option<String>,
    pub edit_index: Option<usize>,
}

impl JumpFormState {
    pub fn new_empty() -> Self {
        Self {
            user: String::new(),
            host: String::new(),
            port: String::new(),
            password: String::new(),
            input: JumpField::Host,
            error: None,
            edit_index: None,
        }
    }

    pub fn from_jump(jump: &JumpHost, index: usize) -> Self {
        Self {
            user: jump.user.clone(),
            host: jump.host.clone(),
            port: jump.port.to_string(),
            password: jump.password.clone().unwrap_or_default(),
            input: JumpField::Host,
            error: None,
            edit_index: Some(index),
        }
    }

    pub fn to_jump(&self) -> Result<JumpHost, String> {
        let host = self.host.trim().to_string();
        if host.is_empty() {
            return Err("Host is required".to_string());
        }
        let port = if self.port.trim().is_empty() {
            22
        } else {
            parse_port(&self.port)?
        };
        let password = {
            let p = self.password.trim().to_string();
            if p.is_empty() {
                None
            } else {
                Some(p)
            }
        };
        Ok(JumpHost {
            user: self.user.trim().to_string(),
            host,
            port,
            password,
        })
    }

    pub fn cycle_input(&mut self) {
        self.input = match self.input {
            JumpField::Host => JumpField::User,
            JumpField::User => JumpField::Port,
            JumpField::Port => JumpField::Password,
            JumpField::Password => JumpField::Host,
        };
    }
}

fn parse_port(s: &str) -> Result<u16, String> {
    let port = s.trim().parse::<u16>().map_err(|_| format!("Invalid port: {}", s.trim()))?;
    if port == 0 {
        return Ok(0);
    }
    Ok(port)
}

pub struct App {
    pub screen: Screen,
    pub list: ListState,
    pub form: FormState,
    pub jump_form: JumpFormState,
    pub config: Config,
    pub store: ConfigStore,
    pub ssh: SshManager,
    pub status: Option<String>,
    pub search: String,
    pub search_mode: bool,
}

impl App {
    pub fn new() -> Self {
        Self::new_with_store(ConfigStore::new())
    }

    pub fn new_with_store(store: ConfigStore) -> Self {
        let config = store.load();
        let mut ssh = SshManager::new();
        let names: Vec<String> = config.tunnels.iter().map(|t| t.name.clone()).collect();
        ssh.refresh_all(&names);
        Self {
            screen: Screen::List,
            list: ListState { selected: 0 },
            form: FormState::new_empty(),
            jump_form: JumpFormState::new_empty(),
            config,
            store,
            ssh,
            status: None,
            search: String::new(),
            search_mode: false,
        }
    }
        pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        match self.screen {
            Screen::List => self.handle_list_key(key),
            Screen::Create | Screen::Edit => self.handle_form_key(key),
            Screen::JumpAdd | Screen::JumpEdit => self.handle_jump_form_key(key),
        }
    }

    pub fn visible_tunnels(&self) -> Vec<usize> {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return (0..self.config.tunnels.len()).collect();
        }
        self.config
            .tunnels
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.name.to_lowercase().contains(&q)
                    || t.target.host.to_lowercase().contains(&q)
                    || t.target.port.to_string().contains(&q)
                    || t.local_port.to_string().contains(&q)
                    || t.jumps.iter().any(|j| {
                        j.host.to_lowercase().contains(&q)
                            || j.user.to_lowercase().contains(&q)
                    })
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn handle_list_key(&mut self, key: crossterm::event::KeyEvent) {
        if self.search_mode {
            match key.code {
                KeyCode::Esc => {
                    self.search.clear();
                    self.search_mode = false;
                }
                KeyCode::Char(c) => {
                    self.search.push(c);
                }
                KeyCode::Backspace => {
                    self.search.pop();
                }
                KeyCode::Enter => {
                    self.search_mode = false;
                }
                _ => {}
            }
            let vis = self.visible_tunnels().len();
            self.list.clamp(vis);
            return;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.list.selected > 0 {
                    self.list.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let vis = self.visible_tunnels().len();
                if vis > 0 && self.list.selected + 1 < vis {
                    self.list.selected += 1;
                }
            }
            KeyCode::Char('/') => {
                self.search.clear();
                self.search_mode = true;
            }
            KeyCode::Enter => {
                let vis = self.visible_tunnels();
                if let Some(&idx) = vis.get(self.list.selected) {
                    let name = self.config.tunnels[idx].name.clone();
                    if self.ssh.is_running(&name) {
                        match self.ssh.stop(&name) {
                            Ok(_) => self.status = Some(format!("Stopped '{name}'")),
                            Err(e) => self.status = Some(format!("Error: {e}")),
                        }
                    } else {
                        let tunnel = self.config.tunnels[idx].clone();
                        match self.ssh.start(&tunnel) {
                            Ok(_) => self.status = Some(format!("Started '{name}'")),
                            Err(e) => self.status = Some(format!("Error: {e}")),
                        }
                    }
                }
            }
            KeyCode::Char('n') => {
                self.form = FormState::new_empty();
                self.status = None;
                self.screen = Screen::Create;
            }
            KeyCode::Char('e') => {
                let vis = self.visible_tunnels();
                if let Some(&idx) = vis.get(self.list.selected) {
                    let tunnel = self.config.tunnels[idx].clone();
                    self.form = FormState::from_tunnel(&tunnel, idx);
                    self.status = None;
                    self.screen = Screen::Edit;
                }
            }
            KeyCode::Char('d') => {
                let vis = self.visible_tunnels();
                if let Some(&idx) = vis.get(self.list.selected) {
                    let name = self.config.tunnels[idx].name.clone();
                    let _ = self.ssh.stop(&name);
                    self.config.tunnels.remove(idx);
                    self.list.clamp(self.visible_tunnels().len());
                    let _ = self.store.save(&self.config);
                    self.status = Some(format!("Deleted '{name}'"));
                }
            }
            KeyCode::Char('q') => {
                if let Err(e) = self.store.save(&self.config) {
                    self.status = Some(format!("Save error: {e}"));
                }
            }
            _ => {}
        }
    }

    fn handle_form_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::List;
                self.status = None;
            }
            KeyCode::Tab => {
                self.form.cycle_input();
            }
            KeyCode::Enter => {
                match self.save_form() {
                    Ok(_) => {
                        self.screen = Screen::List;
                        self.status = Some("Saved.".to_string());
                    }
                    Err(e) => self.form.error = Some(e),
                }
            }
            _ => {
                if self.form.input == InputField::Jumps {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.form.selected_jump > 0 {
                                self.form.selected_jump -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if self.form.selected_jump + 1 < self.form.jumps.len() {
                                self.form.selected_jump += 1;
                            }
                        }
                        KeyCode::Char('a') => {
                            self.jump_form = JumpFormState::new_empty();
                            self.screen = Screen::JumpAdd;
                        }
                        KeyCode::Char('e') => {
                            if !self.form.jumps.is_empty() {
                                let jump = self.form.jumps[self.form.selected_jump].clone();
                                self.jump_form =
                                    JumpFormState::from_jump(&jump, self.form.selected_jump);
                                self.screen = Screen::JumpEdit;
                            }
                        }
                        KeyCode::Char('x') => {
                            if !self.form.jumps.is_empty() {
                                self.form.jumps.remove(self.form.selected_jump);
                                if self.form.selected_jump >= self.form.jumps.len()
                                    && !self.form.jumps.is_empty()
                                {
                                    self.form.selected_jump = self.form.jumps.len() - 1;
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
match key.code {
                        KeyCode::Char(c) => match self.form.input {
                            InputField::Name => self.form.name.push(c),
                            InputField::LocalPort => self.form.local_port.push(c),
                            InputField::TargetHost => self.form.target_host.push(c),
                            InputField::TargetPort => self.form.target_port.push(c),
                            InputField::TargetPassword => self.form.target_password.push(c),
                            InputField::Legacy => self.form.legacy = !self.form.legacy,
                            InputField::Jumps => {}
                        },
                        KeyCode::Backspace => match self.form.input {
                            InputField::Name => {
                                self.form.name.pop();
                            }
                            InputField::LocalPort => {
                                self.form.local_port.pop();
                            }
                            InputField::TargetHost => {
                                self.form.target_host.pop();
                            }
                            InputField::TargetPort => {
                                self.form.target_port.pop();
                            }
                            InputField::TargetPassword => {
                                self.form.target_password.pop();
                            }
                            InputField::Legacy | InputField::Jumps => {}
                        },
                        _ => {}
                    }
                }
            }
        }
    }

    fn handle_jump_form_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.screen = if self.form.is_edit {
                    Screen::Edit
                } else {
                    Screen::Create
                };
            }
            KeyCode::Tab => {
                self.jump_form.cycle_input();
            }
            KeyCode::Enter => {
                match self.jump_form.to_jump() {
                    Ok(jump) => {
                        if let Some(idx) = self.jump_form.edit_index {
                            if idx < self.form.jumps.len() {
                                self.form.jumps[idx] = jump;
                            }
                        } else {
                            self.form.jumps.push(jump);
                        }
                        self.screen = if self.form.is_edit {
                            Screen::Edit
                        } else {
                            Screen::Create
                        };
                    }
                    Err(e) => self.jump_form.error = Some(e),
                }
            }
            KeyCode::Char(c) => match self.jump_form.input {
                JumpField::User => self.jump_form.user.push(c),
                JumpField::Host => self.jump_form.host.push(c),
                JumpField::Port => self.jump_form.port.push(c),
                JumpField::Password => self.jump_form.password.push(c),
            },
            KeyCode::Backspace => match self.jump_form.input {
                JumpField::User => {
                    self.jump_form.user.pop();
                }
                JumpField::Host => {
                    self.jump_form.host.pop();
                }
                JumpField::Port => {
                    self.jump_form.port.pop();
                }
                JumpField::Password => {
                    self.jump_form.password.pop();
                }
            },
            _ => {}
        }
    }

    fn save_form(&mut self) -> Result<(), String> {
        let tunnel = self.form.to_tunnel()?;

        if !self.form.is_edit {
            if self.config.tunnels.iter().any(|t| t.name == tunnel.name) {
                return Err(format!("A tunnel named '{}' already exists", tunnel.name));
            }
            self.config.tunnels.push(tunnel);
        } else {
            let idx = self.form.edit_index;
            if idx >= self.config.tunnels.len() {
                return Err("Internal error: edit index out of bounds".to_string());
            }
            let was_running = self.ssh.is_running(&self.config.tunnels[idx].name);
            if was_running {
                let _ = self.ssh.stop(&self.config.tunnels[idx].name);
            }
            self.config.tunnels[idx] = tunnel;
        }

        self.store.save(&self.config).map_err(|e| e)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    fn temp_app(name: &str) -> App {
        let dir = std::env::temp_dir().join("ssht_cli_tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = ConfigStore::with_path(dir.join("tunnels.json"));
        App::new_with_store(store)
    }

    #[test]
    fn create_tunnel_with_two_jumps() {
        let mut app = temp_app("create_two_jumps");

        // Main list -> create
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.screen, Screen::Create);
        assert_eq!(app.form.input, InputField::Name);

        // Name (contains letters that used to be intercepted: a, e, x)
        type_text(&mut app, "ProdAPI eu-1 a");
        assert_eq!(app.form.name, "ProdAPI eu-1 a");
        app.handle_key(key(KeyCode::Tab)); // -> LocalPort
        assert_eq!(app.form.input, InputField::LocalPort);
        type_text(&mut app, "8443");
        app.handle_key(key(KeyCode::Tab)); // -> Jumps
        assert_eq!(app.form.input, InputField::Jumps);

// Add jump #1 (with a password)
        app.handle_key(key(KeyCode::Char('a')));
        assert_eq!(app.screen, Screen::JumpAdd);
        assert_eq!(app.jump_form.input, JumpField::Host);
        type_text(&mut app, "bastion.example.com");
        app.handle_key(key(KeyCode::Tab)); // -> User
        assert_eq!(app.jump_form.input, JumpField::User);
        type_text(&mut app, "alice");
        app.handle_key(key(KeyCode::Tab)); // -> Port
        assert_eq!(app.jump_form.input, JumpField::Port);
        type_text(&mut app, "2222");
        app.handle_key(key(KeyCode::Tab)); // -> Password
        assert_eq!(app.jump_form.input, JumpField::Password);
        type_text(&mut app, "hunter2");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Create);
        assert_eq!(app.form.jumps.len(), 1);
        assert_eq!(app.form.input, InputField::Jumps);
        assert_eq!(app.form.jumps[0].password.as_deref(), Some("hunter2"));

        // Add jump #2
        app.handle_key(key(KeyCode::Char('a')));
        assert_eq!(app.screen, Screen::JumpAdd);
        type_text(&mut app, "bastion2.example.com");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "ops");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "22");
        app.handle_key(key(KeyCode::Enter)); // skip password
        assert_eq!(app.screen, Screen::Create);
        assert_eq!(app.form.jumps.len(), 2);
        assert_eq!(app.form.jumps[1].password, None);

        // Tab -> TargetHost, type (contains 'a')
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.form.input, InputField::TargetHost);
        type_text(&mut app, "api.internal.example.com");
        assert_eq!(app.form.target_host, "api.internal.example.com");
        // Tab -> TargetPort
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.form.input, InputField::TargetPort);
        type_text(&mut app, "443");
// Tab -> TargetPassword
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.form.input, InputField::TargetPassword);
        type_text(&mut app, "finalloc");
        // Tab -> Legacy algorithms, toggle on with any char
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.form.input, InputField::Legacy);
        app.handle_key(key(KeyCode::Char('t')));
        assert!(app.form.legacy);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.form.input, InputField::Name);

        // Save
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::List);
        assert_eq!(app.config.tunnels.len(), 1);

        let t = &app.config.tunnels[0];
        assert_eq!(t.name, "ProdAPI eu-1 a");
        assert_eq!(t.local_port, 8443);
        assert_eq!(t.target.host, "api.internal.example.com");
        assert_eq!(t.legacy, true);
assert_eq!(t.target.port, 443);
        assert_eq!(t.target.password.as_deref(), Some("finalloc"));
        assert_eq!(t.jumps.len(), 2);
        assert_eq!(t.jumps[0].host, "bastion.example.com");
        assert_eq!(t.jumps[0].user, "alice");
        assert_eq!(t.jumps[0].port, 2222);
        assert_eq!(t.jumps[0].password.as_deref(), Some("hunter2"));
        assert_eq!(t.jumps[1].host, "bastion2.example.com");
        assert_eq!(t.jumps[1].user, "ops");
        assert_eq!(t.jumps[1].port, 22);
        assert_eq!(t.jumps[1].password, None);

        // Verify it was persisted to disk
        let saved = app.store.load();
        assert_eq!(saved.tunnels.len(), 1);
    }

    #[test]
    fn duplicate_name_is_rejected() {
        let mut app = temp_app("duplicate_name");
        app.config.tunnels.push(Tunnel {
            name: "ProdAPI".into(),
            jumps: vec![],
            target: Target {
                host: "h".into(),
                port: 22,
                password: None,
            },
            local_port: 1,
        legacy: false,
        });

        app.handle_key(key(KeyCode::Char('n')));
        type_text(&mut app, "ProdAPI");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Create);
        assert!(app.form.error.is_some());

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::List);
    }

    #[test]
    fn search_filters_list() {
        let mut app = temp_app("search_filter");
        app.config.tunnels = vec![
            Tunnel {
                name: "Alpha".into(),
                jumps: vec![],
                target: Target { host: "a.example.com".into(), port: 22, password: None },
                local_port: 1,
            legacy: false,
            },
            Tunnel {
                name: "Beta".into(),
                jumps: vec![],
                target: Target { host: "b.example.com".into(), port: 22, password: None },
                local_port: 2,
            legacy: false,
            },
        ];

        // enter search mode
        app.handle_key(key(KeyCode::Char('/')));
        assert!(app.search_mode);
        type_text(&mut app, "beta");
        let vis = app.visible_tunnels();
        assert_eq!(vis.len(), 1);
        assert_eq!(app.config.tunnels[vis[0]].name, "Beta");

        // Esc clears search
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.search_mode);
        assert_eq!(app.visible_tunnels().len(), 2);
    }

#[test]
    fn failing_tunnel_captures_output() {
        use std::time::{Duration, Instant};
        // Guard against machines without OpenSSH on PATH.
        if std::process::Command::new("ssh").arg("-V").output().is_err() {
            return;
        }
        let mut app = temp_app("fail_capture");
        app.config.tunnels.push(Tunnel {
            name: "probe".into(),
            jumps: vec![],
            target: Target {
                host: "localhost".into(),
                port: 22,
                password: None,
            },
            local_port: 5678,
        legacy: false,
        });

        // Enter on the list starts the selected tunnel.
        app.handle_key(key(KeyCode::Enter));
        assert!(app.ssh.is_running("probe"));

        // Local sshd (when reachable) rejects keys and gets an empty askpass
        // answer, so ssh should exit quickly and leave us its stderr output.
        let deadline = Instant::now() + Duration::from_secs(25);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
            if !app.ssh.is_running("probe") {
                break;
            }
        }
        let out = app.ssh.output("probe").unwrap_or("").to_string();
        assert!(!out.is_empty(), "expected captured ssh output, got none");
    }

    #[test]
    fn vim_navigation_and_delete() {
        let mut app = temp_app("vim_nav");
        app.config.tunnels = vec![
            Tunnel {
                name: "One".into(),
                jumps: vec![],
                target: Target { host: "1.example.com".into(), port: 22, password: None },
                local_port: 1,
            legacy: false,
            },
            Tunnel {
                name: "Two".into(),
                jumps: vec![],
                target: Target { host: "2.example.com".into(), port: 22, password: None },
                local_port: 2,
            legacy: false,
            },
            Tunnel {
                name: "Three".into(),
                jumps: vec![],
                target: Target { host: "3.example.com".into(), port: 22, password: None },
                local_port: 3,
            legacy: false,
            },
        ];

        // 'j' moves down, 'k' moves up
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.list.selected, 1);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.list.selected, 2);
        app.handle_key(key(KeyCode::Char('j'))); // clamped at bottom
        assert_eq!(app.list.selected, 2);
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.list.selected, 1);

        // 'd' deletes the selected tunnel
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.config.tunnels.len(), 2);
        assert_eq!(app.config.tunnels[0].name, "One");
        assert_eq!(app.config.tunnels[1].name, "Three");
    }
}
