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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputField {
    Name,
    LocalPort,
    KeyPath,
    TargetHost,
    TargetPort,
    Jumps,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JumpField {
    User,
    Host,
    Port,
}

pub struct FormState {
    pub name: String,
    pub local_port: String,
    pub key_path: String,
    pub target_host: String,
    pub target_port: String,
    pub jumps: Vec<JumpHost>,
    pub selected_jump: usize,
    pub input: InputField,
    pub error: Option<String>,
    pub is_edit: bool,
    pub edit_index: usize,
}

impl FormState {
    pub fn new_empty() -> Self {
        Self {
            name: String::new(),
            local_port: String::new(),
            key_path: String::new(),
            target_host: String::new(),
            target_port: String::new(),
            jumps: Vec::new(),
            selected_jump: 0,
            input: InputField::Name,
            error: None,
            is_edit: false,
            edit_index: 0,
        }
    }

    pub fn from_tunnel(tunnel: &Tunnel, index: usize) -> Self {
        Self {
            name: tunnel.name.clone(),
            local_port: tunnel.local_port.to_string(),
            key_path: tunnel.key_path.clone().unwrap_or_default(),
            target_host: tunnel.target.host.clone(),
            target_port: tunnel.target.port.to_string(),
            jumps: tunnel.jumps.clone(),
            selected_jump: 0,
            input: InputField::Name,
            error: None,
            is_edit: true,
            edit_index: index,
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
        let key_path = {
            let k = self.key_path.trim().to_string();
            if k.is_empty() {
                None
            } else {
                Some(k)
            }
        };
        Ok(Tunnel {
            name,
            jumps: self.jumps.clone(),
            target: Target {
                host: target_host,
                port: target_port,
            },
            local_port,
            key_path,
        })
    }

    pub fn cycle_input(&mut self) {
        self.input = match self.input {
            InputField::Name => InputField::LocalPort,
            InputField::LocalPort => InputField::KeyPath,
            InputField::KeyPath => InputField::TargetHost,
            InputField::TargetHost => InputField::TargetPort,
            InputField::TargetPort => InputField::Jumps,
            InputField::Jumps => InputField::Name,
        };
    }
}

pub struct JumpFormState {
    pub user: String,
    pub host: String,
    pub port: String,
    pub input: JumpField,
    pub error: Option<String>,
    pub edit_index: Option<usize>,
}

impl JumpFormState {
    pub fn new_empty() -> Self {
        Self {
            user: String::new(),
            host: String::new(),
            port: String::from("22"),
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
        Ok(JumpHost {
            user: self.user.trim().to_string(),
            host,
            port,
        })
    }

    pub fn cycle_input(&mut self) {
        self.input = match self.input {
            JumpField::User => JumpField::Host,
            JumpField::Host => JumpField::Port,
            JumpField::Port => JumpField::User,
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
    pub next_screen: Option<Screen>,
}

impl App {
    pub fn new() -> Self {
        let store = ConfigStore::new();
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
            next_screen: None,
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        match self.screen {
            Screen::List => self.handle_list_key(key),
            Screen::Create | Screen::Edit => self.handle_form_key(key),
            Screen::JumpAdd | Screen::JumpEdit => self.handle_jump_form_key(key),
        }
    }

    fn handle_list_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if self.list.selected > 0 {
                    self.list.selected -= 1;
                }
            }
            KeyCode::Down => {
                if !self.config.tunnels.is_empty()
                    && self.list.selected + 1 < self.config.tunnels.len()
                {
                    self.list.selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(tunnel) = self.config.tunnels.get(self.list.selected) {
                    let name = tunnel.name.clone();
                    if self.ssh.is_running(&name) {
                        match self.ssh.stop(&name) {
                            Ok(_) => self.status = Some(format!("Stopped '{name}'")),
                            Err(e) => self.status = Some(format!("Error: {e}")),
                        }
                    } else {
                        match self.ssh.start(tunnel) {
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
                if !self.config.tunnels.is_empty() {
                    let tunnel = self.config.tunnels[self.list.selected].clone();
                    self.form = FormState::from_tunnel(&tunnel, self.list.selected);
                    self.status = None;
                    self.screen = Screen::Edit;
                }
            }
            KeyCode::Char('d') => {
                if !self.config.tunnels.is_empty() {
                    let name = self.config.tunnels[self.list.selected].name.clone();
                    let _ = self.ssh.stop(&name);
                    self.config.tunnels.remove(self.list.selected);
                    if self.list.selected >= self.config.tunnels.len() && !self.config.tunnels.is_empty() {
                        self.list.selected = self.config.tunnels.len() - 1;
                    }
                    let _ = self.store.save(&self.config);
                    self.status = Some(format!("Deleted '{name}'"));
                }
            }
            KeyCode::Char('q') => {
                self.status = Some("Saving config...".to_string());
                match self.store.save(&self.config) {
                    Ok(_) => self.status = Some("Bye! Config saved.".to_string()),
                    Err(e) => self.status = Some(format!("Save error: {e}")),
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
            KeyCode::Char('a') => {
                self.jump_form = JumpFormState::new_empty();
                self.next_screen = Some(Screen::JumpAdd);
            }
            KeyCode::Char('x') => {
                if !self.form.jumps.is_empty() {
                    self.form.jumps.remove(self.form.selected_jump);
                    if self.form.selected_jump >= self.form.jumps.len() && !self.form.jumps.is_empty() {
                        self.form.selected_jump = self.form.jumps.len() - 1;
                    }
                }
            }
            KeyCode::Char('e') => {
                if !self.form.jumps.is_empty() {
                    let jump = self.form.jumps[self.form.selected_jump].clone();
                    self.jump_form = JumpFormState::from_jump(&jump, self.form.selected_jump);
                    self.next_screen = Some(Screen::JumpEdit);
                }
            }
            KeyCode::Up => {
                if self.form.selected_jump > 0 {
                    self.form.selected_jump -= 1;
                }
            }
            KeyCode::Down => {
                if !self.form.jumps.is_empty()
                    && self.form.selected_jump + 1 < self.form.jumps.len()
                {
                    self.form.selected_jump += 1;
                }
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
            KeyCode::Char(c) => match self.form.input {
                InputField::Name => self.form.name.push(c),
                InputField::LocalPort => self.form.local_port.push(c),
                InputField::KeyPath => self.form.key_path.push(c),
                InputField::TargetHost => self.form.target_host.push(c),
                InputField::TargetPort => self.form.target_port.push(c),
            },
            KeyCode::Backspace => match self.form.input {
                InputField::Name => {
                    self.form.name.pop();
                }
                InputField::LocalPort => {
                    self.form.local_port.pop();
                }
                InputField::KeyPath => {
                    self.form.key_path.pop();
                }
                InputField::TargetHost => {
                    self.form.target_host.pop();
                }
                InputField::TargetPort => {
                    self.form.target_port.pop();
                }
            },
            _ => {}
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
