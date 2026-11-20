//! Геометрия карты Лаборатории: проекция игровых координат на тайлы tarkov.dev,
//! этажи и комнаты под карты доступа.

use serde::Deserialize;

pub const MAP_ID: &str = "the-lab";
pub const TILE_ROOT: &str = "https://assets.tarkov.dev/maps/labs_v4";
/// Зум тайлов, из которых склеиваем подложку (256px * 2^Z на всю сетку).
pub const TILE_ZOOM: u32 = 4;

/// Этажи слева направо: подвал, первый, второй.
pub const FLOORS: [Floor; 3] = [
    Floor { tile_dir: "technical", title: "Технический этаж" },
    Floor { tile_dir: "1st", title: "1 этаж" },
    Floor { tile_dir: "2nd", title: "2 этаж" },
];

pub struct Floor {
    pub tile_dir: &'static str,
    pub title: &'static str,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Pos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Номер этажа по высоте (данные tarkov.dev: heightRange [-0.9, 3],
/// второй этаж — выше 3 и только в пределах своих границ).
pub fn floor_of(p: Pos) -> usize {
    if p.y < -0.9 {
        0
    } else if p.y >= 3.0 && (-271.0..=-101.0).contains(&p.x) && (-422.0..=-270.0).contains(&p.z) {
        2
    } else {
        1
    }
}

/// Нормированные координаты сетки тайлов (0..1 на всю сетку зума 0).
/// transform [0.575, 281.2, 0.575, 193.7], поворот 270°, размер тайла 175.
pub fn grid_uv(x: f64, z: f64) -> (f64, f64) {
    let u = (0.575 * z + 281.2) / 175.0;
    let v = (0.575 * x + 193.7) / 175.0;
    (u, v)
}

/// Комната за дверью: многоугольник в игровых координатах [x, z].
#[derive(Clone, Debug)]
pub struct Room {
    pub name: String,
    pub key: String,
    pub floor: usize,
    pub poly: Vec<[f64; 2]>,
    pub approx: bool,
    /// Комнаты нет на подложке tarkov.dev — дорисовываем её в картинку этажа.
    pub solid: bool,
    /// Точка [x, z], откуда брать текстуру пола для дорисовки.
    pub texture_from: Option<[f64; 2]>,
}

/// Формат файла: либо `poly` (из редактора зон), либо старый прямоугольник `x`/`z`.
#[derive(Deserialize)]
struct RawRoom {
    name: String,
    key: String,
    floor: usize,
    #[serde(default)]
    poly: Vec<[f64; 2]>,
    x: Option<[f64; 2]>,
    z: Option<[f64; 2]>,
    #[serde(default)]
    approx: bool,
    #[serde(default)]
    solid: bool,
    texture_from: Option<[f64; 2]>,
}

impl Room {
    pub fn contains(&self, p: Pos, floor: usize) -> bool {
        floor == self.floor && point_in_poly(&self.poly, p.x, p.z)
    }

    pub fn center(&self) -> (f64, f64) {
        let n = self.poly.len().max(1) as f64;
        let (sx, sz) = self.poly.iter().fold((0.0, 0.0), |(a, b), p| (a + p[0], b + p[1]));
        (sx / n, sz / n)
    }
}

pub fn point_in_poly(poly: &[[f64; 2]], x: f64, z: f64) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (xi, zi) = (poly[i][0], poly[i][1]);
        let (xj, zj) = (poly[j][0], poly[j][1]);
        if (zi > z) != (zj > z) && x < (xj - xi) * (z - zi) / (zj - zi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Зоны комнат за дверями.
pub fn rooms() -> Vec<Room> {
    // dev: файл из исходников (его правит редактор зон);
    // релиз: свежая копия из репозитория (кэш), иначе вшитая.
    let dev = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/labs_rooms.json"));
    let cached = crate::data::data_dir().join("labs_rooms.json");
    let text = [cfg!(debug_assertions).then_some(dev), Some(cached)]
        .into_iter()
        .flatten()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| include_str!("../assets/labs_rooms.json").to_string());
    let raw: Vec<RawRoom> = serde_json::from_str(&text).unwrap_or_default();
    raw.into_iter()
        .map(|r| {
            let poly = if r.poly.len() >= 3 {
                r.poly
            } else {
                let (x, z) = (r.x.unwrap_or_default(), r.z.unwrap_or_default());
                vec![[x[0], z[0]], [x[1], z[0]], [x[1], z[1]], [x[0], z[1]]]
            };
            Room { name: r.name, key: r.key, floor: r.floor, poly, approx: r.approx, solid: r.solid, texture_from: r.texture_from }
        })
        .collect()
}

/// Цвет ключа/карты по его id (RGB).
pub fn key_color(key_id: &str) -> u32 {
    match key_id {
        "5c1d0efb86f7744baf2e7b7b" => 0xe53935, // Red
        "5c1d0dc586f7744baf2e7b79" => 0x43a047, // Green
        "5c1d0c5f86f7744bb2683cf0" => 0x1e88e5, // Blue
        "5c1e495a86f7743109743dfb" => 0x8e24aa, // Violet
        "5c1d0d6d86f7744bb2683e1f" => 0xfdd835, // Yellow
        "5c1d0f4986f7744bb01837fa" => 0x212121, // Black
        "5efde6b4f5448336730dbd61" => 0x4fc3f7, // синяя метка
        "6711039f9e648049e50b3307" => 0xffffff, // жилой блок
        _ => 0x9e9e9e,                          // обычные ключи
    }
}

/// Подтягивает свежую разметку зон из репозитория (в релизе), чтобы правки
/// зон доходили без выпуска новой версии.
pub fn refresh_rooms_from_repo() {
    if cfg!(debug_assertions) {
        return;
    }
    let url = format!("https://raw.githubusercontent.com/{}/main/assets/labs_rooms.json", crate::updater::REPO);
    if let Ok(bytes) = crate::data::http_get(&url) {
        if serde_json::from_slice::<Vec<RawRoom>>(&bytes).is_ok() {
            let _ = std::fs::write(crate::data::data_dir().join("labs_rooms.json"), bytes);
        }
    }
}
