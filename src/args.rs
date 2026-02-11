use crate::config::Config;
use clap::{ArgAction, Args};
use std::path::PathBuf;

#[derive(Debug, Clone, Args)]
pub struct CommonArgs {
    /// Path to a config.toml file
    #[arg(short = 'c', long, value_name = "PATH", env = "RUSTYCLAW_CONFIG")]
    pub config: Option<PathBuf>,
    /// Settings directory (OpenClaw-compatible)
    #[arg(long, value_name = "DIR", env = "RUSTYCLAW_SETTINGS_DIR")]
    pub settings_dir: Option<PathBuf>,
    /// Path to SOUL.md
    #[arg(long, value_name = "PATH", env = "RUSTYCLAW_SOUL")]
    pub soul: Option<PathBuf>,
    /// Skills directory
    #[arg(long = "skills", value_name = "DIR", env = "RUSTYCLAW_SKILLS")]
    pub skills_dir: Option<PathBuf>,
    /// Disable secrets storage
    #[arg(long = "no-secrets", action = ArgAction::SetTrue)]
    pub no_secrets: bool,
    /// Gateway WebSocket URL (ws://...)
    #[arg(long = "gateway", alias = "url", alias = "ws", value_name = "WS_URL", env = "RUSTYCLAW_GATEWAY")]
    pub gateway: Option<String>,
}

impl CommonArgs {
    pub fn config_path(&self) -> Option<PathBuf> {
        if let Some(config) = &self.config {
            return Some(config.clone());
        }

        if let Some(settings_dir) = &self.settings_dir {
            return Some(settings_dir.join("config.toml"));
        }

        None
    }

    pub fn apply_overrides(&self, config: &mut Config) {
        if let Some(settings_dir) = &self.settings_dir {
            config.settings_dir = settings_dir.clone();
        }

        if let Some(soul) = &self.soul {
            config.soul_path = Some(soul.clone());
        }

        if let Some(skills_dir) = &self.skills_dir {
            config.skills_dir = Some(skills_dir.clone());
        }

        if self.no_secrets {
            config.use_secrets = false;
        }

        if let Some(gateway) = &self.gateway {
            config.gateway_url = Some(gateway.clone());
        }
    }
}
