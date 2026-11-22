//! Настройки, которые переживают перезапуск: settings.json в папке данных.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::data::data_dir;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "pve" | "pvp"
    pub mode: String,
    /// Язык названий из игры: "ru" | "en" (интерфейс — русский).
    pub lang: String,
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
    /// Точки появления ЧВК на карте.
    pub show_spawns: bool,
    pub auto_refresh: bool,
    /// Проверять обновления приложения при запуске.
    pub auto_update_check: bool,
    /// Открытый раздел: "maps" | "tasks" | "scanner" | "raids" | "settings".
    pub section: String,
    /// Выбранная карта (id из maps.rs).
    pub map: String,
    pub scanner: ScannerSettings,
}

/// Настройки сканера цен (оверлей поверх игры, только Windows).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScannerSettings {
    pub enabled: bool,
    /// Клавиша сканирования предмета под курсором.
    pub scan_key: String,
    /// Клавиша скрытия оверлея.
    pub hide_key: String,
    /// Скан всего контейнера под курсором → окно «После рейда».
    pub raid_key: String,
    /// Реагировать на клавиши, только когда впереди окно игры.
    pub only_in_game: bool,
    /// Сохранять снимки сканов в папку данных — для отладки распознавания.
    pub debug_shots: bool,
    pub show_flea: bool,
    pub show_trader: bool,
    pub show_per_slot: bool,
    /// Нужно ли для убежища и задач.
    pub show_needed: bool,
    /// Показывать оверлей, только пока держу клавишу сканирования.
    /// На оверлей после рейда не действует — он висит до подтверждения.
    pub hold_to_show: bool,
    pub hide_on_mouse_move: bool,
    /// Сколько секунд держать оверлей; 0 — пока не скрою клавишей.
    pub show_seconds: u32,
    /// "cursor" | "corner"
    pub position: String,
}

impl Default for ScannerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            scan_key: "F9".into(),
            hide_key: "F10".into(),
            raid_key: "F8".into(),
            only_in_game: true,
            debug_shots: false,
            show_flea: true,
            show_trader: true,
            show_per_slot: true,
            show_needed: true,
            hold_to_show: false,
            hide_on_mouse_move: true,
            show_seconds: 5,
            position: "cursor".into(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: "pve".into(),
            lang: "ru".into(),
            threshold: 100_000,
            hidden_items: BTreeSet::new(),
            hidden_groups: BTreeSet::new(),
            hidden_access: BTreeSet::new(),
            expanded: BTreeSet::new(),
            collapsed: BTreeSet::new(),
            floor_view: None,
            show_doors: true,
            show_rooms: true,
            show_spawns: true,
            show_extracts: true,
            auto_refresh: true,
            auto_update_check: true,
            section: "maps".into(),
            map: "the-lab".into(),
            scanner: ScannerSettings::default(),
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
