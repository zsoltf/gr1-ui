use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    White,
    #[default]
    Dark,
}

impl Theme {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "light" | "white" => Ok(Self::White),
            "dark" => Ok(Self::Dark),
            _ => Err("Theme must be light or dark (white is also accepted)".into()),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Dark => "dark",
        }
    }
}

pub struct Appearance {
    pub theme: Theme,
    pub brightness: u8,
    path: Option<PathBuf>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            brightness: 50,
            path: None,
        }
    }
}

impl Appearance {
    /// `device_file` is the appearance file of the instrument's data layout.
    pub fn load(device_file: Option<&Path>) -> Self {
        // UI preferences never enter an engine setting or a GRP/GRM file.
        let path = std::env::var_os("GR1_UI_SETTINGS")
            .map(PathBuf::from)
            .or_else(|| device_file.map(Path::to_path_buf))
            .or_else(|| {
                if cfg!(target_os = "macos") {
                    std::env::var_os("HOME").map(|home| {
                        PathBuf::from(home)
                            .join("Library/Application Support/GR-1 UI/appearance.json")
                    })
                } else {
                    std::env::var_os("XDG_CONFIG_HOME")
                        .map(PathBuf::from)
                        .or_else(|| {
                            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
                        })
                        .map(|root| root.join("gr1-ui/appearance.json"))
                }
            });
        Self::load_path(path)
    }

    fn load_path(path: Option<PathBuf>) -> Self {
        let saved = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        let theme = saved
            .as_ref()
            .and_then(|value| Theme::parse(value.get("theme")?.as_str()?).ok())
            .unwrap_or_default();
        let brightness = saved
            .as_ref()
            .and_then(|value| value.get("brightness")?.as_u64())
            .map(|value| value.clamp(10, 100) as u8)
            .unwrap_or_else(|| Self::default().brightness);
        Self {
            theme,
            brightness,
            path,
        }
    }

    pub fn select(&mut self, theme: Theme) -> Result<(), String> {
        self.theme = theme;
        self.save()
    }

    pub fn dim(&mut self, brightness: u8) -> Result<(), String> {
        self.brightness = brightness.clamp(10, 100);
        self.save()
    }

    fn save(&self) -> Result<(), String> {
        let path = self.path.as_ref().ok_or("No UI settings directory")?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temporary = path.with_extension("json.tmp");
        let contents = format!(
            "{{\"theme\":\"{}\",\"brightness\":{}}}\n",
            self.theme.name(),
            self.brightness
        );
        std::fs::write(&temporary, contents).map_err(|e| e.to_string())?;
        std::fs::rename(temporary, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_start_is_dark_at_half_brightness_and_saved_choices_win() {
        let root = std::env::temp_dir().join(format!("gr1-appearance-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("appearance.json");
        let first = Appearance::load_path(Some(path.clone()));
        assert_eq!(first.theme, Theme::Dark);
        assert_eq!(first.brightness, 50);
        assert!(!path.exists());

        std::fs::write(&path, br#"{"theme":"light","brightness":75}"#).unwrap();
        let saved = Appearance::load_path(Some(path.clone()));
        assert_eq!(saved.theme, Theme::White);
        assert_eq!(saved.brightness, 75);

        std::fs::write(&path, b"{}").unwrap();
        let empty = Appearance::load_path(Some(path));
        assert_eq!(empty.theme, Theme::Dark);
        assert_eq!(empty.brightness, 50);
        std::fs::remove_dir_all(root).unwrap();
    }
}
