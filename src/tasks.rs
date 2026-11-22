//! Задания торговцев из дампа tarkov.dev: что сделать, где и с какими ключами, точки целей на карте.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::labs::Pos;
use crate::lang::Lang;

#[derive(Clone, Debug)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub trader: &'static str,
    pub level: u32,
    pub kappa: bool,
    pub lightkeeper: bool,
    /// "USEC" / "BEAR", None — для всех.
    pub faction: Option<String>,
    pub wiki: String,
    /// Карта задания (id приложения), если оно привязано к одной локации.
    pub map: Option<String>,
    /// Названия заданий, которые нужно выполнить раньше.
    pub requires: Vec<String>,
    /// Ключи: карта → имена ключей.
    pub keys: Vec<(String, Vec<String>)>,
    pub objectives: Vec<Objective>,
}

#[derive(Clone, Debug)]
pub struct Objective {
    /// Что сделать — текст цели из игры.
    pub text: String,
    pub optional: bool,
    /// Карты, на которых можно выполнить цель (пусто — у торговца).
    pub maps: Vec<String>,
    /// Подробности: предметы, количество, ключи.
    pub details: Vec<String>,
    pub points: Vec<ObjPoint>,
}

/// Зона цели или место квестового предмета.
#[derive(Clone, Debug)]
pub struct ObjPoint {
    pub map: String,
    pub pos: Pos,
    /// Контур зоны в игровых [x, z]; пусто — просто точка.
    pub outline: Vec<[f64; 2]>,
}

impl Task {
    /// Привязано к этой карте целиком.
    pub fn is_local(&self, map: &str) -> bool {
        self.map.as_deref() == Some(map)
            || (!self.objectives.iter().any(|o| o.maps.iter().any(|m| m != map))
                && self.objectives.iter().any(|o| o.maps.iter().any(|m| m == map)))
    }

    /// Хоть одну цель можно выполнить на этой карте.
    pub fn touches(&self, map: &str) -> bool {
        self.map.as_deref() == Some(map) || self.objectives.iter().any(|o| o.maps.iter().any(|m| m == map))
    }

    pub fn points_on<'a>(&'a self, map: &'a str) -> impl Iterator<Item = (usize, &'a ObjPoint)> + 'a {
        self.objectives
            .iter()
            .enumerate()
            .flat_map(move |(i, o)| o.points.iter().filter(move |p| p.map == map).map(move |p| (i, p)))
    }
}

/// Имя торговца как в игре.
pub fn trader_name(id: &str, lang: Lang) -> &'static str {
    let (ru, en) = match id {
        "54cb50c76803fa8b248b4571" => ("Прапор", "Prapor"),
        "54cb57776803fa99248b456e" => ("Терапевт", "Therapist"),
        "579dc571d53a0658a154fbec" => ("Скупщик", "Fence"),
        "58330581ace78e27b8b10cee" => ("Лыжник", "Skier"),
        "5935c25fb3acc3127c3d8cd9" => ("Миротворец", "Peacekeeper"),
        "5a7c2eca46aef81a7ca2145d" => ("Механик", "Mechanic"),
        "5ac3b934156ae10c4430e83c" => ("Барахольщик", "Ragman"),
        "5c0647fdd443bc2504c2d371" => ("Егерь", "Jaeger"),
        "6617beeaa9cfa777ca915b7c" => ("Реф", "Ref"),
        "638f541a29ffd1183d187f57" => ("Смотритель", "Lightkeeper"),
        "656f0f98d80a697f855d34b1" => ("Водитель БТР", "BTR Driver"),
        _ => ("Торговец", "Trader"),
    };
    lang.pick(ru, en)
}

fn values(v: &Value) -> Vec<&Value> {
    match v {
        Value::Object(m) => m.values().collect(),
        Value::Array(a) => a.iter().collect(),
        _ => Vec::new(),
    }
}

fn pos(v: &Value) -> Option<Pos> {
    Some(Pos { x: v["x"].as_f64()?, y: v["y"].as_f64().unwrap_or(0.0), z: v["z"].as_f64()? })
}

/// Читает задания из кэша режима. `item_names` — переводы предметов на языке `lang` («<id> Name»),
/// `map_names` — id карты в дампе → её normalizedName.
pub fn load(dir: &Path, item_names: &HashMap<String, String>, map_names: &HashMap<String, String>, lang: Lang) -> Vec<Task> {
    let read = |name: &str| -> Option<Value> {
        let v: Value = serde_json::from_slice(&std::fs::read(dir.join(name)).ok()?).ok()?;
        v.get("data").cloned()
    };
    let Some(data) = read("tasks.json") else { return Vec::new() };
    let names_tr: HashMap<String, String> = read(&format!("tasks_{}.json", lang.code()))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let tr = |key: &str| names_tr.get(key).cloned();
    let quest_items = data.get("questItems").cloned().unwrap_or(Value::Null);
    let item_name = |id: &str| -> String {
        item_names
            .get(&format!("{id} Name"))
            .cloned()
            .or_else(|| quest_items.get(id).and_then(|q| q["name"].as_str()).and_then(&tr))
            .unwrap_or_else(|| id.to_string())
    };
    let map_of = |id: &str| -> Option<String> {
        map_names.get(id).map(|n| crate::maps::canonical(n).to_string())
    };
    let maps_of = |v: &Value| -> Vec<String> {
        let mut out: Vec<String> = v.as_array().into_iter().flatten().filter_map(|m| m.as_str()).filter_map(&map_of).collect();
        out.sort();
        out.dedup();
        out
    };

    let list = data.get("tasks").cloned().unwrap_or(Value::Null);
    let raw = values(&list);
    let names: HashMap<&str, String> = raw
        .iter()
        .filter_map(|t| {
            let id = t["id"].as_str()?;
            Some((id, tr(t["name"].as_str().unwrap_or(id)).unwrap_or_else(|| id.to_string())))
        })
        .collect();

    raw.iter()
        .filter_map(|t| {
            let id = t["id"].as_str()?.to_string();
            let objectives = t["objectives"].as_array().into_iter().flatten().map(|o| objective(o, lang, &tr, &item_name, &map_of, &maps_of)).collect();
            let keys = t["neededKeys"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|k| {
                    let map = map_of(k["map"].as_str()?)?;
                    let names: Vec<String> = k["keys"].as_array()?.iter().filter_map(|i| i.as_str()).map(&item_name).collect();
                    (!names.is_empty()).then_some((map, names))
                })
                .collect();
            let requires = t["taskRequirements"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| names.get(r["task"].as_str()?).cloned())
                .collect();
            Some(Task {
                name: names.get(id.as_str()).cloned().unwrap_or_else(|| id.clone()),
                trader: trader_name(t["trader"].as_str().unwrap_or(""), lang),
                level: t["minPlayerLevel"].as_u64().unwrap_or(0) as u32,
                kappa: t["kappaRequired"].as_bool().unwrap_or(false),
                lightkeeper: t["lightkeeperRequired"].as_bool().unwrap_or(false),
                faction: t["factionName"].as_str().filter(|f| *f != "Any").map(str::to_string),
                wiki: t["wikiLink"].as_str().unwrap_or("").to_string(),
                map: t["map"].as_str().and_then(&map_of),
                requires,
                keys,
                objectives,
                id,
            })
        })
        .collect()
}

fn objective(
    o: &Value,
    lang: Lang,
    tr: &dyn Fn(&str) -> Option<String>,
    item_name: &dyn Fn(&str) -> String,
    map_of: &dyn Fn(&str) -> Option<String>,
    maps_of: &dyn Fn(&Value) -> Vec<String>,
) -> Objective {
    let kind = o["type"].as_str().unwrap_or("");
    let count = o["count"].as_u64().unwrap_or(1);
    let fir = o["foundInRaid"].as_bool().unwrap_or(false);
    let mut details = Vec::new();
    // игровые термины — на языке названий, как в клиенте
    let t = |ru: &'static str, en: &'static str| lang.pick(ru, en);
    let or = t(" или ", " or ");
    let fir_text = if fir { t(" · найдено в рейде", " · found in raid") } else { "" };

    // предметы: «любой из 50» не расписываем — это видно из текста
    let items: Vec<String> = o["items"].as_array().into_iter().flatten().filter_map(|i| i.as_str()).map(item_name).collect();
    if !items.is_empty() && items.len() <= 3 {
        details.push(format!(
            "{}{}{fir_text}",
            items.join(or),
            if count > 1 { format!(" × {count}") } else { String::new() },
        ));
    } else if count > 1 && matches!(kind, "giveItem" | "findItem" | "sellItem" | "plantItem") {
        details.push(format!("{}: {count}{fir_text}", t("Количество", "Count")));
    } else if count > 1 && matches!(kind, "shoot" | "extract" | "experience" | "useItem") {
        details.push(format!("{}: {count}", t("Сколько раз", "Times")));
    }
    if let Some(q) = o["questItem"].as_str() {
        details.push(format!("{}: {}", t("Предмет задания", "Quest item"), item_name(q)));
    }
    if let Some(m) = o["markerItem"].as_str() {
        details.push(format!("{}: {}", t("Чем отметить", "Mark with"), item_name(m)));
    }
    for alts in o["requiredKeys"].as_array().into_iter().flatten() {
        let names: Vec<String> = alts.as_array().into_iter().flatten().filter_map(|k| k.as_str()).map(item_name).collect();
        if !names.is_empty() {
            details.push(format!("🔑 {}: {}", t("Нужен ключ", "Key required"), names.join(or)));
        }
    }
    if let Some(d) = o["distance"]["value"].as_f64().filter(|d| *d > 0.0) {
        let more = o["distance"]["compareMethod"].as_str() != Some("<=");
        details.push(match (lang, more) {
            (Lang::Ru, more) => format!("Дистанция {} {d:.0} м", if more { "от" } else { "до" }),
            (Lang::En, true) => format!("Distance: {d:.0} m or more"),
            (Lang::En, false) => format!("Distance: up to {d:.0} m"),
        });
    }
    let (from, until) = (o["timeFromHour"].as_u64().unwrap_or(0), o["timeUntilHour"].as_u64().unwrap_or(0));
    if from != until {
        details.push(match lang {
            Lang::Ru => format!("Время в игре: с {from}:00 до {until}:00"),
            Lang::En => format!("In-game time: {from}:00–{until}:00"),
        });
    }

    // зоны и места квестовых предметов; у версий одной карты точки совпадают — сливаем
    let mut points: Vec<ObjPoint> = Vec::new();
    let mut push = |map: Option<String>, pos: Option<Pos>, outline: Vec<[f64; 2]>| {
        let (Some(map), Some(pos)) = (map, pos) else { return };
        if points.iter().any(|p| p.map == map && (p.pos.x - pos.x).abs() < 1.0 && (p.pos.z - pos.z).abs() < 1.0) {
            return;
        }
        points.push(ObjPoint { map, pos, outline });
    };
    for z in o["zones"].as_array().into_iter().flatten() {
        let outline = z["outline"].as_array().into_iter().flatten().filter_map(pos).map(|p| [p.x, p.z]).collect();
        push(z["map"].as_str().and_then(map_of), pos(&z["position"]), outline);
    }
    for loc in o["possibleLocations"].as_array().into_iter().flatten() {
        let map = loc["map"].as_str().and_then(map_of);
        for p in loc["positions"].as_array().into_iter().flatten() {
            push(map.clone(), pos(p), Vec::new());
        }
    }

    let desc = o["description"].as_str().unwrap_or("");
    Objective {
        text: tr(desc).unwrap_or_else(|| desc.to_string()),
        optional: o["optional"].as_bool().unwrap_or(false),
        maps: maps_of(&o["maps"]),
        details,
        points,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{data_dir, Mode};

    /// На реальном кэше: у заданий Эпицентра есть русские тексты и точки на карте.
    #[test]
    fn ground_zero_tasks() {
        let dir = data_dir().join(Mode::Pve.dir());
        if !dir.join("tasks.json").exists() {
            return;
        }
        let ru: HashMap<String, String> = serde_json::from_slice::<Value>(&std::fs::read(dir.join("items_ru.json")).unwrap())
            .ok()
            .and_then(|v| serde_json::from_value(v["data"].clone()).ok())
            .unwrap_or_default();
        let mut names = HashMap::new();
        names.insert("653e6760052c01c1c805532f".to_string(), "ground-zero".to_string());
        names.insert("65b8d6f5cdde2479cb2a3125".to_string(), "ground-zero-21".to_string());
        let tasks = load(&dir, &ru, &names, Lang::Ru);
        assert!(tasks.len() > 100);
        let gz: Vec<&Task> = tasks.iter().filter(|t| t.is_local(crate::maps::GROUND_ZERO)).collect();
        assert!(gz.len() >= 5, "{}", gz.len());
        assert!(gz.iter().any(|t| t.points_on(crate::maps::GROUND_ZERO).next().is_some()));
        // тексты переведены, а не id
        assert!(gz.iter().all(|t| t.objectives.iter().all(|o| !o.text.chars().all(|c| c.is_ascii_hexdigit()))));
    }
}
