use std::path::Path;
use std::fs;

use crate::error::{FerrixError, Result};
use super::Config;

pub struct ConfigParser {
    config: Config,
}

impl ConfigParser {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    pub fn parse_file(&mut self, path: &Path) -> Result<()> {
        let contents = fs::read_to_string(path)
            .map_err(|e| FerrixError::Config(format!("Failed to read config file: {}", e)))?;

        self.parse_string(&contents)
    }

    pub fn parse_string(&mut self, contents: &str) -> Result<()> {
        self.config = toml::from_str(contents)
            .map_err(|e| FerrixError::Config(format!("Failed to parse config: {}", e)))?;

        Ok(())
    }

    pub fn get_config(self) -> Config {
        self.config
    }

    pub fn validate(&self) -> Result<()> {
        // Validate configuration values
        if self.config.status_bar.height == 0 || self.config.status_bar.height > 5 {
            return Err(FerrixError::Config("Status bar height must be between 1 and 5".to_string()));
        }

        if self.config.windows.base_index > 99 {
            return Err(FerrixError::Config("Window base index must be less than 100".to_string()));
        }

        if self.config.panes.base_index > 99 {
            return Err(FerrixError::Config("Pane base index must be less than 100".to_string()));
        }

        Ok(())
    }
}