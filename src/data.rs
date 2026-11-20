//! Загрузка полного дампа json.tarkov.dev, кэш на диске и разбор в модель.

use anyhow::{Context as _, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::categories;
use crate::labs::{self, Pos, Room};

pub const USER_AGENT: &str = "tarkov-helper/0.1";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Pve,
    Pvp,
}

impl Mode {
    pub fn dir(self) -> &'static str {
        match self {
            Mode::Pve => "pve",
            Mode::Pvp => "regular",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Mode::Pve => "PvE",
            Mode::Pvp => "PvP",
        }
    }
}

/// Файлы, которые качаем целиком (полный дамп режима).
const DUMP_FILES: &[&str] = &[
    "items", "items_ru", "items_en", "maps", "maps_ru", "traders", "barters", "crafts", "tasks", "tasks_ru",
    "hideout", "hideout_ru",
];

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("tarkov-helper")
}

pub fn http_get(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

/// Скачивает полный дамп режима в кэш. Файлы пишутся атомарно (через .tmp).
pub fn download_dump(mode: Mode) -> Result<()> {
    let dir = data_dir().join(mode.dir());
    std::fs::create_dir_all(&dir)?;
    let results: Vec<Result<()>> = std::thread::scope(|s| {
        let handles: Vec<_> = DUMP_FILES
            .iter()
            .map(|name| {
                let dir = dir.clone();
                s.spawn(move || -> Result<()> {
                    let bytes = http_get(&format!("https://json.tarkov.dev/{}/{name}", mode.dir()))?;
                    let tmp = dir.join(format!("{name}.json.tmp"));
                    std::fs::write(&tmp, bytes)?;
                    std::fs::rename(tmp, dir.join(format!("{name}.json")))?;
                    Ok(())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for r in results {
        r?;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    std::fs::write(dir.join("fetched_at"), now.to_string())?;
    Ok(())
}

pub fn has_dump(mode: Mode) -> bool {
    let dir = data_dir().join(mode.dir());
    ["items", "items_ru", "maps"].iter().all(|f| dir.join(format!("{f}.json")).exists())
}

// ---------- сырые структуры json.tarkov.dev ----------

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemsData {
    items: HashMap<String, RawItem>,
    #[serde(default)]
    item_categories: HashMap<String, RawCategory>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCategory {
    normalized_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawItem {
    id: String,
    last_low_price: Option<f64>,
    #[serde(default)]
    sell_to_trader: Vec<RawSell>,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Deserialize)]
struct RawSell {
    trader: String,
    #[serde(rename = "priceRUB")]
    price_rub: Option<f64>,
}

#[derive(Deserialize)]
struct MapsData {
    maps: HashMap<String, RawMap>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMap {
    normalized_name: String,
    #[serde(default)]
    loot_loose: Vec<RawLoose>,
    #[serde(default)]
    locks: Vec<RawLock>,
    #[serde(default)]
    extracts: Vec<RawExtract>,
    #[serde(default)]
    transits: Vec<RawTransit>,
}

#[derive(Deserialize)]
struct RawExtract {
    name: String,
    #[serde(default)]
    faction: String,
    position: Pos,
}

#[derive(Deserialize)]
struct RawTransit {
    position: Pos,
}

#[derive(Deserialize)]
struct RawLoose {
    position: Pos,
    items: Vec<String>,
}

#[derive(Deserialize)]
struct RawLock {
    key: Option<String>,
    position: Pos,
}

// ---------- модель ----------

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub short: String,
    /// Короткое имя в английской игре — по нему тоже узнаём ярлык в инвентаре.
    pub short_en: String,
    /// lastLowPrice на барахолке (0, если продать нельзя / нет предложений).
    pub flea: i64,
    /// Лучшая цена продажи торговцу.
    pub trader: i64,
    pub trader_name: &'static str,
    /// Индекс подкатегории в `categories::GROUPS`.
    pub group: usize,
    pub width: u32,
    pub height: u32,
    /// Строка для поиска: ру/англ полные и короткие имена, в нижнем регистре, ё→е.
    pub search: String,
}

impl Item {
    pub fn slots(&self) -> u32 {
        (self.width * self.height).max(1)
    }

    pub fn price(&self) -> i64 {
        self.flea.max(self.trader)
    }
    pub fn price_source(&self) -> &'static str {
        if self.flea >= self.trader { "Барахолка" } else { self.trader_name }
    }
}

#[derive(Clone, Debug)]
pub struct LootPoint {
    pub pos: Pos,
    pub floor: usize,
    pub items: Vec<String>,
    /// Индекс комнаты в `LabData::rooms`, если точка за запертой дверью.
    pub room: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Lock {
    pub pos: Pos,
    pub floor: usize,
    pub key: String,
}

#[derive(Clone, Debug)]
pub struct Extract {
    pub name: String,
    /// pmc / scav / shared / transit
    pub faction: String,
    pub pos: Pos,
    pub floor: usize,
}

/// Зачем предмет нужен: уровень убежища или задача.
#[derive(Clone, Debug)]
pub struct Need {
    pub hideout: bool,
    pub name: String,
    pub count: u32,
    pub found_in_raid: bool,
}

pub struct LabData {
    pub mode: Mode,
    pub fetched_at: Option<u64>,
    pub items: HashMap<String, Item>,
    pub loot: Vec<LootPoint>,
    pub locks: Vec<Lock>,
    pub extracts: Vec<Extract>,
    pub rooms: Vec<Room>,
    /// item id -> где нужен.
    pub needs: HashMap<String, Vec<Need>>,
}

impl LabData {
    /// Короткое имя, как говорят игроки: LEDX, GPU, 0.2BTC.
    pub fn item_short(&self, id: &str) -> String {
        self.items.get(id).map(|i| i.short.clone()).unwrap_or_else(|| id.to_string())
    }
}

fn trader_name(id: &str) -> &'static str {
    match id {
        "54cb50c76803fa8b248b4571" => "Прапор",
        "54cb57776803fa99248b456e" => "Терапевт",
        "579dc571d53a0658a154fbec" => "Скупщик",
        "58330581ace78e27b8b10cee" => "Лыжник",
        "5935c25fb3acc3127c3d8cd9" => "Миротворец",
        "5a7c2eca46aef81a7ca2145d" => "Механик",
        "5ac3b934156ae10c4430e83c" => "Барахольщик",
        "5c0647fdd443bc2504c2d371" => "Егерь",
        "6617beeaa9cfa777ca915b7c" => "Реф",
        _ => "Торговец",
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

/// Читает кэш режима и собирает данные Лабы.
pub fn load_lab(mode: Mode) -> Result<LabData> {
    let dir = data_dir().join(mode.dir());
    let items: Envelope<ItemsData> = read_json(&dir.join("items.json"))?;
    let ru: Envelope<HashMap<String, String>> = read_json(&dir.join("items_ru.json"))?;
    // английские имена — для поиска; файла может не быть в старом кэше
    let en: HashMap<String, String> = read_json::<Envelope<HashMap<String, String>>>(&dir.join("items_en.json"))
        .map(|e| e.data)
        .unwrap_or_default();
    let maps: Envelope<MapsData> = read_json(&dir.join("maps.json"))?;
    let fetched_at = std::fs::read_to_string(dir.join("fetched_at")).ok().and_then(|s| s.trim().parse().ok());

    let cats = items.data.item_categories;
    let items: HashMap<String, Item> = items
        .data
        .items
        .into_values()
        .map(|raw| {
            let no_flea = raw.types.iter().any(|t| t == "noFlea");
            let flea = if no_flea { 0 } else { raw.last_low_price.unwrap_or(0.0) as i64 };
            let best = raw
                .sell_to_trader
                .iter()
                .filter_map(|s| s.price_rub.map(|p| (p as i64, s.trader.as_str())))
                .max_by_key(|(p, _)| *p);
            let name = ru.data.get(&format!("{} Name", raw.id)).cloned().unwrap_or_else(|| raw.id.clone());
            let short = ru.data.get(&format!("{} ShortName", raw.id)).cloned().unwrap_or_else(|| name.clone());
            let cat = raw.categories.first().and_then(|c| cats.get(c)).map(|c| c.normalized_name.as_str());
            let en_name = en.get(&format!("{} Name", raw.id)).map(String::as_str).unwrap_or("");
            let en_short = en.get(&format!("{} ShortName", raw.id)).map(String::as_str).unwrap_or("");
            let search = normalize(&format!("{name}|{short}|{en_name}|{en_short}"));
            let item = Item {
                id: raw.id.clone(),
                name,
                short,
                short_en: en_short.to_string(),
                flea,
                trader: best.map(|b| b.0).unwrap_or(0),
                trader_name: best.map(|b| trader_name(b.1)).unwrap_or(""),
                group: categories::group_of(cat),
                width: raw.width.unwrap_or(1),
                height: raw.height.unwrap_or(1),
                search,
            };
            (raw.id, item)
        })
        .collect();

    let lab = maps
        .data
        .maps
        .into_values()
        .find(|m| m.normalized_name == labs::MAP_ID)
        .context("в дампе нет карты the-lab")?;

    let rooms = labs::rooms();
    let loot = lab
        .loot_loose
        .into_iter()
        .map(|l| {
            let floor = labs::floor_of(l.position);
            let room = rooms.iter().position(|r| r.contains(l.position, floor));
            LootPoint { pos: l.position, floor, items: l.items, room }
        })
        .collect();
    let locks = lab
        .locks
        .into_iter()
        .filter_map(|l| Some(Lock { floor: labs::floor_of(l.position), pos: l.position, key: l.key? }))
        .collect();

    let maps_ru: HashMap<String, String> = read_json::<Envelope<HashMap<String, String>>>(&dir.join("maps_ru.json"))
        .map(|e| e.data)
        .unwrap_or_default();
    let mut extracts: Vec<Extract> = lab
        .extracts
        .into_iter()
        .map(|e| Extract {
            name: maps_ru.get(&e.name).cloned().unwrap_or(e.name),
            faction: e.faction,
            floor: labs::floor_of(e.position),
            pos: e.position,
        })
        .collect();
    extracts.extend(lab.transits.into_iter().map(|t| Extract {
        name: "Переход".into(),
        faction: "transit".into(),
        floor: labs::floor_of(t.position),
        pos: t.position,
    }));

    let needs = load_needs(&dir);

    Ok(LabData { mode, fetched_at, items, loot, locks, extracts, rooms, needs })
}

pub fn normalize(s: &str) -> String {
    s.to_lowercase().replace('ё', "е")
}

pub fn fmt_rub(v: i64) -> String {
    let s = v.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    format!("{}{out} ₽", if v < 0 { "-" } else { "" })
}

pub fn fmt_short(v: i64) -> String {
    if v >= 1_000_000 {
        format!("{:.1}M", v as f64 / 1e6)
    } else if v >= 1000 {
        format!("{}k", v / 1000)
    } else {
        v.to_string()
    }
}

/// Требования убежища и задач по предметам. Файлов может не быть — тогда пусто.
fn load_needs(dir: &Path) -> HashMap<String, Vec<Need>> {
    use serde_json::Value;
    let read = |name: &str| -> Option<Value> {
        let v: Value = serde_json::from_slice(&std::fs::read(dir.join(name)).ok()?).ok()?;
        v.get("data").cloned()
    };
    let tr = |v: &Option<Value>, key: &str| -> String {
        v.as_ref().and_then(|t| t.get(key)).and_then(|s| s.as_str()).unwrap_or(key).to_string()
    };
    let values = |v: &Value| -> Vec<Value> {
        match v {
            Value::Object(m) => m.values().cloned().collect(),
            Value::Array(a) => a.clone(),
            _ => Vec::new(),
        }
    };
    let mut needs: HashMap<String, Vec<Need>> = HashMap::new();

    if let Some(hideout) = read("hideout.json") {
        let names = read("hideout_ru.json");
        let stations = hideout.get("hideoutStations").cloned().unwrap_or(hideout);
        for st in values(&stations) {
            let station = tr(&names, st["name"].as_str().unwrap_or(""));
            for lv in st["levels"].as_array().into_iter().flatten() {
                let level = lv["level"].as_u64().unwrap_or(0);
                for r in lv["itemRequirements"].as_array().into_iter().flatten() {
                    let Some(item) = r["item"].as_str() else { continue };
                    needs.entry(item.to_string()).or_default().push(Need {
                        hideout: true,
                        name: format!("{station}, ур. {level}"),
                        count: r["count"].as_u64().unwrap_or(1) as u32,
                        found_in_raid: r["attributes"]["foundInRaid"].as_bool().unwrap_or(false),
                    });
                }
            }
        }
    }

    if let Some(tasks) = read("tasks.json") {
        let names = read("tasks_ru.json");
        let list = tasks.get("tasks").cloned().unwrap_or(tasks);
        for task in values(&list) {
            let name = tr(&names, task["name"].as_str().unwrap_or(""));
            for o in task["objectives"].as_array().into_iter().flatten() {
                if o["type"].as_str() != Some("giveItem") {
                    continue;
                }
                // «сдать любой из 30 предметов» — не про конкретный предмет
                let items = o["items"].as_array().cloned().unwrap_or_default();
                if items.len() > 3 {
                    continue;
                }
                for it in items.iter().filter_map(|i| i.as_str()) {
                    needs.entry(it.to_string()).or_default().push(Need {
                        hideout: false,
                        name: name.clone(),
                        count: o["count"].as_u64().unwrap_or(1) as u32,
                        found_in_raid: o["foundInRaid"].as_bool().unwrap_or(false),
                    });
                }
            }
        }
    }
    needs
}
