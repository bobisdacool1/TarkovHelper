//! Настройки, которые переживают перезапуск: settings.json в папке данных.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::categories::{CASES, OTHER};
use crate::data::data_dir;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "pve" | "pvp"
    pub mode: String,
    /// Язык названий из игры: "ru" | "en" (интерфейс — русский).
    pub lang: String,
    /// Вид карты лута: "classic" — самое ценное на точке и фильтр по цене предмета,
    /// "chances" — шансы SPT и средняя сумма точки за рейд.
    pub loot_view: String,
    /// Классический вид: показывать предметы дороже этого.
    pub threshold: i64,
    /// Вид «шансы»: минимальная средняя сумма точки за рейд (шанс × цена по видимым предметам).
    pub min_ev: i64,
    /// Вид «шансы», точки лута: "tarkov" — tarkov.dev с шансами SPT, "spt" — целиком SPT.
    pub loot_source: String,
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
    /// Показанные на карте типы контейнеров (id шаблона). По умолчанию — ни одного.
    pub shown_containers: BTreeSet<String>,
    pub auto_refresh: bool,
    /// Проверять обновления приложения при запуске.
    pub auto_update_check: bool,
    /// Размер иконок на карте, % (один из `ICON_SIZES`).
    pub icon_size: u32,
    /// Открытый раздел: "maps" | "tasks" | "scanner" | "raids" | "settings".
    pub section: String,
    /// Выбранная карта (id из maps.rs).
    pub map: String,
    pub scanner: ScannerSettings,
    /// Сохранённые наборы фильтров карты.
    pub presets: Vec<FilterPreset>,
    /// Фильтры уже знают, что «Контейнеры» выделены из «Прочего».
    pub cases_split: bool,
}

/// Размеры иконок на карте, %; 100 — базовый.
pub const ICON_SIZES: &[u32] = &[75, 100, 125, 150, 200];

/// Размер из настроек; незнакомое значение (правили файл руками) — базовый.
pub fn icon_size(saved: u32) -> u32 {
    if ICON_SIZES.contains(&saved) { saved } else { 100 }
}

/// Пресет фильтров: галочки категорий и предметов, доступ, порог и поиск под своим именем.
#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FilterPreset {
    pub name: String,
    pub threshold: i64,
    pub min_ev: i64,
    pub search: String,
    pub hidden_items: BTreeSet<String>,
    pub hidden_groups: BTreeSet<usize>,
    pub hidden_access: BTreeSet<String>,
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
    /// На оверлей после рейда не действует — он висит до подтверждения, отмены или клика мимо.
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
            loot_view: "classic".into(),
            threshold: 100_000,
            min_ev: 5_000,
            loot_source: "tarkov".into(),
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
            shown_containers: BTreeSet::new(),
            auto_refresh: true,
            auto_update_check: true,
            icon_size: 100,
            section: "maps".into(),
            map: "the-lab".into(),
            scanner: ScannerSettings::default(),
            presets: Vec::new(),
            cases_split: false,
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
        .map(split_cases)
        .unwrap_or_else(|| split_cases(Settings::default()))
}

/// Кейсы раньше были в «Прочем»: где оно скрыто, скрываем и «Контейнеры» — один раз.
fn split_cases(mut s: Settings) -> Settings {
    if !s.cases_split {
        let presets = s.presets.iter_mut().map(|p| &mut p.hidden_groups);
        for hidden in std::iter::once(&mut s.hidden_groups).chain(presets) {
            if hidden.contains(&OTHER) {
                hidden.insert(CASES);
            }
        }
        s.cases_split = true;
    }
    s
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Settings {
        split_cases(serde_json::from_str(json).unwrap())
    }

    #[test]
    fn icon_size_defaults() {
        assert_eq!(parse("{}").icon_size, 100);
        assert_eq!(icon_size(150), 150);
        assert_eq!(icon_size(0), 100);
        assert_eq!(icon_size(9000), 100);
    }

    #[test]
    fn cases_follow_hidden_other_once() {
        let old = parse(r#"{"hidden_groups":[9],"presets":[{"hidden_groups":[9]},{"hidden_groups":[1]}]}"#);
        assert_eq!(old.hidden_groups, BTreeSet::from([OTHER, CASES]));
        assert_eq!(old.presets[0].hidden_groups, BTreeSet::from([OTHER, CASES]));
        assert_eq!(old.presets[1].hidden_groups, BTreeSet::from([1]));
        // потом «Контейнеры» включили сами — не прячем снова
        let new = parse(r#"{"hidden_groups":[9],"cases_split":true}"#);
        assert_eq!(new.hidden_groups, BTreeSet::from([OTHER]));
        assert!(parse("{}").hidden_groups.is_empty());
    }
}
