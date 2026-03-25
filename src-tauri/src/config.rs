use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub env_vars: Vec<(String, String)>,
    pub health_check_url: Option<String>,
    pub health_check_interval_secs: u64,
    pub auto_restart: bool,
    pub log_path: String,
    pub enabled: bool,
    #[serde(default)]
    pub last_pid: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProcessConfig {
    pub fn new(name: String, command: String) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            command,
            args: Vec::new(),
            working_dir: None,
            env_vars: Vec::new(),
            health_check_url: None,
            health_check_interval_secs: 600,
            auto_restart: true,
            log_path: String::new(),
            enabled: true,
            last_pid: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub processes: Vec<ProcessConfig>,
}

pub fn get_config_dir() -> anyhow::Result<PathBuf> {
    let project_dirs = directories::ProjectDirs::from("com", "rsigild", "rsigild")
        .ok_or_else(|| anyhow::anyhow!("Failed to get project directories"))?;
    let config_dir = project_dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir)
}

pub fn get_config_path() -> anyhow::Result<PathBuf> {
    Ok(get_config_dir()?.join("config.json"))
}

pub fn load_config() -> anyhow::Result<AppConfig> {
    let config_path = get_config_path()?;
    if !config_path.exists() {
        return Ok(AppConfig {
            processes: Vec::new(),
        });
    }
    let content = std::fs::read_to_string(config_path)?;
    let config: AppConfig = serde_json::from_str(&content)?;
    Ok(config)
}

pub fn save_config(config: &AppConfig) -> anyhow::Result<()> {
    let config_path = get_config_path()?;
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(config_path, content)?;
    Ok(())
}
