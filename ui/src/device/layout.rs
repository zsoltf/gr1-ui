//! Where the instrument's data lives under the data root. Detected once at
//! startup; every slot, location and host-file path comes from here.
use std::path::{Path, PathBuf};

/// The host's own folder on a legacy root. Stock reads performances only
/// from `performanceNN` folders, so nothing in here is read as one.
pub const LEGACY_HOST_DIR: &str = "gr1-ui-settings";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Firmware 3.x: `<root>/user/perf/performanceNN`, `<root>/user`.
    User,
    /// Firmware 2.8.x: `<root>/performanceNN` directly under the root.
    Legacy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    root: PathBuf,
    pub kind: Kind,
}

impl Layout {
    /// `<root>/user/perf` wins; otherwise `<root>/performance01` means the
    /// legacy layout; otherwise the current layout (its reads then fail on
    /// the missing `user/perf` slot, naming that path).
    pub fn detect(root: &Path) -> Self {
        let kind = if root.join("user/perf").is_dir() {
            Kind::User
        } else if root.join("performance01").is_dir() {
            Kind::Legacy
        } else {
            Kind::User
        };
        Self {
            root: root.into(),
            kind,
        }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// The folder of user slot `index` (0..50), `performance{index+1:02}`.
    pub fn slot(&self, index: usize) -> PathBuf {
        let name = format!("performance{:02}", index + 1);
        match self.kind {
            Kind::User => self.root.join("user/perf").join(name),
            Kind::Legacy => self.root.join(name),
        }
    }
    pub fn slots(&self) -> Vec<PathBuf> {
        (0..50).map(|i| self.slot(i)).collect()
    }
    /// The Internal location: user data, where Files starts.
    pub fn internal(&self) -> PathBuf {
        match self.kind {
            Kind::User => self.root.join("user"),
            Kind::Legacy => self.root.clone(),
        }
    }
    /// The read-only Factory location.
    pub fn factory(&self) -> PathBuf {
        self.root.join("factory")
    }
    /// The engine/system settings file. On the current layout this is the
    /// stock 3.4.4 file it shares with the stock application; a legacy root
    /// has none, so the host keeps its own in its folder.
    pub fn system_settings(&self) -> PathBuf {
        match self.kind {
            Kind::User => self.root.join("system_settings.ini"),
            Kind::Legacy => self.root.join(LEGACY_HOST_DIR).join("system_settings.ini"),
        }
    }
    /// The UI appearance preference (theme, brightness).
    pub fn appearance(&self) -> PathBuf {
        let legacy = self.root.join(LEGACY_HOST_DIR).join("appearance.json");
        match self.kind {
            Kind::User => {
                let current = self.root.join("ui-appearance.json");
                // Stock's 3.x migration moves performances, but not our preferences.
                if !current.exists() && legacy.is_file() {
                    legacy
                } else {
                    current
                }
            }
            Kind::Legacy => legacy,
        }
    }
}
