// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const CONFIG_DIR_NAME: &str = "io.github.tkilian.CosmicAppletWorkspaceWindows";
const LEGACY_CONFIG_DIR_NAME: &str = "io.github.tkilian.CosmicAppletAppTitle";
const CONFIG_FILE_NAME: &str = "config.toml";

pub const DEFAULT_TITLE_CHARS: usize = 24;
pub const MIN_TITLE_CHARS: usize = 8;
pub const MAX_TITLE_CHARS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppletConfig {
    pub limit_tile_size: bool,
    pub max_title_chars: usize,
    pub middle_click_closes: bool,
    pub show_app_icons: bool,
    pub show_hover_close_button: bool,
}

impl Default for AppletConfig {
    fn default() -> Self {
        Self {
            limit_tile_size: false,
            max_title_chars: DEFAULT_TITLE_CHARS,
            middle_click_closes: true,
            show_app_icons: true,
            show_hover_close_button: true,
        }
    }
}

impl AppletConfig {
    pub fn load() -> Self {
        for path in [
            config_path(CONFIG_DIR_NAME),
            config_path(LEGACY_CONFIG_DIR_NAME),
        ]
        .into_iter()
        .flatten()
        {
            match fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str::<Self>(&contents) {
                    Ok(config) => return config.normalized(),
                    Err(err) => {
                        tracing::warn!("Failed to parse config at {}: {err}", path.display());
                        return Self::default();
                    }
                },
                Err(err) if err.kind() == ErrorKind::NotFound => {}
                Err(err) => {
                    tracing::warn!("Failed to read config at {}: {err}", path.display());
                    return Self::default();
                }
            }
        }

        Self::default()
    }

    pub fn save(&self) {
        let Some(path) = config_path(CONFIG_DIR_NAME) else {
            return;
        };

        let Some(parent) = path.parent() else {
            return;
        };

        if let Err(err) = fs::create_dir_all(parent) {
            tracing::warn!(
                "Failed to create config directory {}: {err}",
                parent.display()
            );
            return;
        }

        let normalized = self.clone().normalized();
        let contents = match toml::to_string_pretty(&normalized) {
            Ok(contents) => contents,
            Err(err) => {
                tracing::warn!("Failed to serialize config: {err}");
                return;
            }
        };

        if let Err(err) = fs::write(&path, contents) {
            tracing::warn!("Failed to write config at {}: {err}", path.display());
        }
    }

    pub fn normalized(mut self) -> Self {
        self.limit_tile_size = false;
        self.middle_click_closes = true;
        self.show_app_icons = true;
        self.max_title_chars = self.max_title_chars.clamp(MIN_TITLE_CHARS, MAX_TITLE_CHARS);
        self
    }
}

fn config_path(dir_name: &str) -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push(Path::new(dir_name));
    path.push(Path::new(CONFIG_FILE_NAME));
    Some(path)
}
