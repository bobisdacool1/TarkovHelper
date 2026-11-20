//! Настройки, которые переживают перезапуск: settings.json в папке данных.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::data::data_dir;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "pve" | "pvp"
    pub mode: String,
    pub threshold: i64,
    pub hidden_items: BTreeSet<String>,
    pub hidden_groups: BTreeSet<usize>,
    /// Скрытый доступ: id ключа, "" — лут без ключа.
    pub hidden_access: BTreeSet<String>,
    pub expanded: BTreeSet<usize>,
    pub collapsed: BTreeSet<String>,
    pub floor_view: Option<usize>,
    pub show_doors: bool,
    pub show_rooms: bool,
    pub show_extracts: bool,
    pub auto_refresh: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: "pve".into(),
            threshold: 100_000,
            hidden_items: BTreeSet::new(),
            hidden_groups: BTreeSet::new(),
            hidden_access: BTreeSet::new(),
            expanded: BTreeSet::new(),
            collapsed: BTreeSet::new(),
            floor_view: None,
            show_doors: true,
            show_rooms: true,
            show_extracts: true,
            auto_refresh: true,
        }
    }
}

fn path() -> std::path::PathBuf {
    data_dir().join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read(path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) {
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(json) = serde_json::to_vec_pretty(s) {
        let tmp = path().with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, path());
        }
    }
}
