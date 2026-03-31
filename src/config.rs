use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub bind: String,
    pub auth_token: String,
    /// All file_args must resolve under this directory. Canonicalized at load time.
    pub file_root: String,
    pub commands: Vec<CommandConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CommandConfig {
    pub name: String,
    pub binary: String,
    pub description: String,
    pub allowed_args: Vec<ArgPattern>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ArgPattern {
    pub pattern: String,
    #[serde(default)]
    pub file_args: Vec<String>,
}

fn default_timeout() -> u64 {
    30
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: Config =
            serde_yaml::from_str(&contents).context("Failed to parse config YAML")?;

        // Validate file_root
        let file_root = Path::new(&config.file_root);
        if !file_root.is_absolute() {
            anyhow::bail!("file_root must be an absolute path, got: {}", config.file_root);
        }
        if !file_root.is_dir() {
            anyhow::bail!("file_root directory does not exist: {}", config.file_root);
        }
        // Canonicalize to resolve any symlinks
        let canonical_root = file_root
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize file_root: {}", config.file_root))?;
        let mut config = config;
        config.file_root = canonical_root.to_string_lossy().into_owned();

        // Validate commands
        for cmd in &config.commands {
            if !Path::new(&cmd.binary).is_absolute() {
                anyhow::bail!(
                    "Command '{}' binary must be an absolute path, got: {}",
                    cmd.name,
                    cmd.binary
                );
            }
        }

        Ok(config)
    }

    pub fn find_command(&self, name: &str) -> Option<&CommandConfig> {
        self.commands.iter().find(|c| c.name == name)
    }
}
