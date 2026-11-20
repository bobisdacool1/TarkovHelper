//! Журнал рейдов и статистика по картам: raids.json в папке данных.
//! Рейд записывается из оверлея после рейда: распознанный рюкзак минус
//! исключённые предметы, плюс карта.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::data::data_dir;

/// Вынесенный предмет с ценой на момент подтверждения рейда.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaidItem {
    pub id: String,
    pub short: String,
    pub count: u32,
    /// Цена за штуку, ₽.
    pub price: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Raid {
    /// Unix-время подтверждения.
    pub ts: u64,
    /// id карты из maps.rs.
    pub map: String,
    /// "pve" | "pvp"
    pub mode: String,
    pub items: Vec<RaidItem>,
}

impl Raid {
    /// Стоимость вынесенного, ₽.
    pub fn total(&self) -> i64 {
        self.items.iter().map(|i| i.price * i.count as i64).sum()
    }
}

fn path() -> std::path::PathBuf {
    data_dir().join("raids.json")
}

pub fn load() -> Vec<Raid> {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(raids: &[Raid]) {
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(json) = serde_json::to_vec_pretty(raids) {
        let tmp = path().with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, path());
        }
    }
}

/// Сводка по одной карте.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapStats {
    pub raids: usize,
    pub total: i64,
    pub best: i64,
}

impl MapStats {
    /// Средний профит за рейд.
    pub fn avg(&self) -> i64 {
        if self.raids == 0 { 0 } else { self.total / self.raids as i64 }
    }
}

/// Статистика по картам; `mode` = None — все режимы.
pub fn stats(raids: &[Raid], mode: Option<&str>) -> BTreeMap<String, MapStats> {
    let mut out: BTreeMap<String, MapStats> = BTreeMap::new();
    for r in raids.iter().filter(|r| mode.is_none_or(|m| r.mode == m)) {
        let s = out.entry(r.map.clone()).or_default();
        s.raids += 1;
        s.total += r.total();
        s.best = s.best.max(r.total());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raid(map: &str, mode: &str, prices: &[(i64, u32)]) -> Raid {
        Raid {
            ts: 0,
            map: map.into(),
            mode: mode.into(),
            items: prices
                .iter()
                .map(|&(price, count)| RaidItem { id: "x".into(), short: "x".into(), count, price })
                .collect(),
        }
    }

    #[test]
    fn total_counts_stacks() {
        assert_eq!(raid("the-lab", "pve", &[(100_000, 1), (5_000, 3)]).total(), 115_000);
    }

    #[test]
    fn averages_by_map_and_mode() {
        let raids = vec![
            raid("the-lab", "pve", &[(1_000_000, 1)]),
            raid("the-lab", "pve", &[(200_000, 1)]),
            raid("the-lab", "pvp", &[(5_000_000, 1)]),
            raid("customs", "pve", &[(100_000, 1)]),
        ];
        let pve = stats(&raids, Some("pve"));
        assert_eq!(pve["the-lab"].raids, 2);
        assert_eq!(pve["the-lab"].avg(), 600_000);
        assert_eq!(pve["the-lab"].best, 1_000_000);
        assert_eq!(stats(&raids, None)["the-lab"].raids, 3);
    }
}
