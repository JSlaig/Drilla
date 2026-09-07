use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpHost {
    pub user: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tunnel {
    pub name: String,
    #[serde(default)]
    pub jumps: Vec<JumpHost>,
    pub target: Target,
    #[serde(default = "default_local_port")]
    pub local_port: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub legacy: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub tunnels: Vec<Tunnel>,
}

fn default_port() -> u16 {
    22
}

fn default_local_port() -> u16 {
    0
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Config {
    pub fn empty() -> Self {
        Self {
            tunnels: Vec::new(),
        }
    }
}
