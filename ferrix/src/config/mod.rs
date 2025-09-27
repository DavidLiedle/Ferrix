// Configuration module placeholder
// Will be expanded with TOML config parsing

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub default_shell: String,
    pub escape_key: String,
    pub mouse: bool,
    pub clipboard: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                default_shell: "/bin/bash".to_string(),
                escape_key: "ctrl-b".to_string(),
                mouse: true,
                clipboard: true,
            },
        }
    }
}