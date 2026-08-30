//! On-disk state: API keys, preferences, favorites and the active numbers.
//!
//! Stored as JSON at `$GATEWAVE_CONFIG`, else `$XDG_CONFIG_HOME/gatewave/config.json`,
//! else `~/.config/gatewave/config.json`. Keys missing from the file are seeded from the
//! environment (`HERO_SMS_API_KEY` …) and from a `.env` file in the working directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::backend::ProviderKind;
use crate::domain::{Favorite, Number, Prefs};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub keys: BTreeMap<ProviderKind, String>,
    pub prefs: Prefs,
    pub favorites: Vec<Favorite>,
    pub numbers: Vec<Number>,
    /// Next local number id.
    pub next_number_id: u32,
}

impl Config {
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("GATEWAVE_CONFIG")
            && !p.is_empty()
        {
            return PathBuf::from(p);
        }
        let base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| Path::new(&h).join(".config"))
            })
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("gatewave").join("config.json")
    }

    /// Loads the config, falling back to defaults when the file is missing or unreadable.
    /// A config written by the app under its previous name (`number-desk`) is picked up when
    /// the new location is still empty; the next save moves it over.
    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists()
            && let Some(legacy) = Self::legacy_path(&path)
            && legacy.exists()
        {
            return Self::load_from(&legacy);
        }
        Self::load_from(&path)
    }

    /// `…/gatewave/config.json` → `…/number-desk/config.json`; `None` for explicit paths.
    fn legacy_path(path: &Path) -> Option<PathBuf> {
        let dir = path.parent()?;
        (dir.file_name()? == "gatewave")
            .then(|| dir.with_file_name("number-desk").join("config.json"))
    }

    pub fn load_from(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!(
                        "gatewave: ignoring unreadable config {}: {e}",
                        path.display()
                    );
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(tmp, path)
    }

    /// Fills in keys that are absent from the config from the process environment and from a
    /// `.env` file in the working directory. Returns the providers that were seeded.
    pub fn seed_keys_from_env(&mut self) -> Vec<ProviderKind> {
        let dotenv = read_dotenv(Path::new(".env"));
        self.seed_keys(|var| {
            std::env::var(var)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| dotenv.get(var).cloned())
        })
    }

    fn seed_keys(&mut self, lookup: impl Fn(&str) -> Option<String>) -> Vec<ProviderKind> {
        let mut seeded = Vec::new();
        for kind in ProviderKind::ALL {
            if self.keys.get(&kind).is_some_and(|k| !k.trim().is_empty()) {
                continue;
            }
            if let Some(key) = lookup(kind.env_key()) {
                self.keys.insert(kind, key.trim().to_owned());
                seeded.push(kind);
            }
        }
        seeded
    }
}

/// Minimal `.env` reader: `KEY=value` lines, `#` comments, optional surrounding quotes.
pub fn read_dotenv(path: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(text) = fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            out.insert(k.trim().to_owned(), v.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("gatewave-test-{}", std::process::id()));
        let path = dir.join("nested").join("config.json");
        let mut cfg = Config::default();
        cfg.keys.insert(ProviderKind::HeroSms, "hk_test".into());
        cfg.prefs.auto_copy = true;
        cfg.next_number_id = 42;
        cfg.save_to(&path).unwrap();
        let back = Config::load_from(&path);
        assert_eq!(back, cfg);
        assert_eq!(
            Config::load_from(&dir.join("missing.json")),
            Config::default()
        );
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(Config::load_from(&path), Config::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_config_dir_is_read_until_the_new_one_exists() {
        let base = std::env::temp_dir().join(format!("gatewave-legacy-{}", std::process::id()));
        let new = base.join("gatewave").join("config.json");
        let old = base.join("number-desk").join("config.json");
        assert_eq!(Config::legacy_path(&new), Some(old.clone()));
        assert_eq!(Config::legacy_path(Path::new("/tmp/custom.json")), None);
        let cfg = Config {
            next_number_id: 7,
            ..Default::default()
        };
        cfg.save_to(&old).unwrap();
        assert!(!new.exists());
        // Same lookup `load()` does, with the paths spelled out.
        let loaded = if new.exists() {
            Config::load_from(&new)
        } else {
            Config::load_from(&Config::legacy_path(&new).unwrap())
        };
        assert_eq!(loaded, cfg);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn seeds_only_missing_keys() {
        let mut cfg = Config::default();
        cfg.keys.insert(ProviderKind::FiveSim, "existing".into());
        let seeded = cfg.seed_keys(|var| match var {
            "HERO_SMS_API_KEY" => Some(" hk_1 ".into()),
            "FIVESIM_API_KEY" => Some("ignored".into()),
            _ => None,
        });
        assert_eq!(seeded, vec![ProviderKind::HeroSms]);
        assert_eq!(cfg.keys[&ProviderKind::HeroSms], "hk_1");
        assert_eq!(cfg.keys[&ProviderKind::FiveSim], "existing");
    }

    #[test]
    fn dotenv_parsing() {
        let dir = std::env::temp_dir().join(format!("gatewave-dotenv-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(".env");
        fs::write(&p, "# comment\nA=1\nexport B=\"two\"\nC='3'\nbad line\n").unwrap();
        let m = read_dotenv(&p);
        assert_eq!(m.get("A").map(String::as_str), Some("1"));
        assert_eq!(m.get("B").map(String::as_str), Some("two"));
        assert_eq!(m.get("C").map(String::as_str), Some("3"));
        assert_eq!(m.len(), 3);
        let _ = fs::remove_dir_all(&dir);
    }
}
