//! Список карт и то, чем они различаются: этажи, подложка, проекция координат.

use crate::labs::{self, Pos};

pub const LAB: &str = "the-lab";
pub const LIGHTHOUSE: &str = "lighthouse";
pub const GROUND_ZERO: &str = "ground-zero";
pub const INTERCHANGE: &str = "interchange";
pub const SHORELINE: &str = "shoreline";
pub const CUSTOMS: &str = "customs";
pub const WOODS: &str = "woods";
pub const RESERVE: &str = "reserve";
pub const STREETS: &str = "streets-of-tarkov";
pub const FACTORY: &str = "factory";
pub const LABYRINTH: &str = "the-labyrinth";
pub const ICEBREAKER: &str = "icebreaker";

pub struct MapInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub available: bool,
}

pub const MAPS: &[MapInfo] = &[
    MapInfo { id: LAB, name: "Лаборатория", available: true },
    MapInfo { id: CUSTOMS, name: "Таможня", available: true },
    MapInfo { id: WOODS, name: "Лес", available: true },
    MapInfo { id: SHORELINE, name: "Берег", available: true },
    MapInfo { id: INTERCHANGE, name: "Развязка", available: true },
    MapInfo { id: RESERVE, name: "Резерв", available: true },
    MapInfo { id: LIGHTHOUSE, name: "Маяк", available: true },
    MapInfo { id: STREETS, name: "Улицы Таркова", available: true },
    MapInfo { id: FACTORY, name: "Завод", available: true },
    MapInfo { id: GROUND_ZERO, name: "Эпицентр", available: true },
    MapInfo { id: LABYRINTH, name: "Лабиринт", available: true },
    MapInfo { id: ICEBREAKER, name: "Ледокол", available: true },
];

/// Карта по id; неизвестная или недоступная — Лаба.
pub fn get(id: &str) -> &'static MapInfo {
    MAPS.iter().find(|m| m.id == id && m.available).unwrap_or(&MAPS[0])
}

pub fn name(id: &str) -> &'static str {
    MAPS.iter().find(|m| m.id == id).map(|m| m.name).unwrap_or("?")
}

/// Карта в дампе tarkov.dev. У Эпицентра две версии: до 21 уровня и 21+ — берём 21+,
/// на ней играет большинство (лута больше, расположение то же).
pub fn dump_name(map: &str) -> &str {
    match map {
        GROUND_ZERO => "ground-zero-21",
        m => m,
    }
}

/// Карта из дампа → карта приложения (версии одной локации сливаются).
pub fn canonical(dump_name: &str) -> &str {
    match dump_name {
        "ground-zero-21" | "ground-zero-tutorial" => GROUND_ZERO,
        "night-factory" => FACTORY,
        m => m,
    }
}

/// Заголовки этажей слева направо. Одноэтажные карты — один безымянный этаж.
pub fn floor_titles(map: &str) -> &'static [&'static str] {
    match map {
        LAB => &["Технический этаж", "1 этаж", "2 этаж"],
        GROUND_ZERO => &["Гараж", "Улица", "2 этаж", "3 этаж"],
        INTERCHANGE => &["Улица и парковка", "1 этаж ТЦ", "2 этаж ТЦ"],
        SHORELINE => &["Подвал санатория", "Улица и 1 этаж", "2 этаж санатория", "3 этаж санатория"],
        CUSTOMS => &["Подвалы", "Улица и 1 этаж", "2 этаж", "3 этаж и выше"],
        FACTORY => &["Подвал", "1 этаж", "2 этаж", "3 этаж"],
        STREETS => &["Подземка и парковки", "Улица и 1 этаж", "2 этаж", "3 этаж", "4 этаж и выше"],
        RESERVE => &["Бункеры", "Улица и 1 этаж", "2 этаж", "3 этаж", "4 этаж и выше"],
        ICEBREAKER => &[
            "Машинное отделение",
            "Склад и охрана",
            "Главная палуба",
            "Спортзал и столовая",
            "Жилой блок",
            "Мостик",
        ],
        _ => &[""],
    }
}

/// Короткие подписи этажей для переключателя.
pub fn floor_tabs(map: &str) -> &'static [&'static str] {
    match map {
        LAB => &["Подвал", "1 этаж", "2 этаж"],
        INTERCHANGE => &["Улица", "1 этаж ТЦ", "2 этаж ТЦ"],
        SHORELINE => &["Подвал", "Улица", "2 этаж", "3 этаж"],
        CUSTOMS => &["Подвалы", "Улица", "2 этаж", "3 этаж+"],
        STREETS => &["Подземка", "Улица", "2 этаж", "3 этаж", "4 этаж+"],
        RESERVE => &["Бункеры", "Улица", "2 этаж", "3 этаж", "4 этаж+"],
        ICEBREAKER => &["Машинное", "Склад", "Палуба", "Столовая", "Жилой блок", "Мостик"],
        m => floor_titles(m),
    }
}

/// Этаж точки.
pub fn floor_of(map: &str, p: Pos) -> usize {
    match map {
        LAB => labs::floor_of(p),
        GROUND_ZERO => ground_zero_floor(p),
        INTERCHANGE => interchange_floor(p),
        SHORELINE => shoreline_floor(p),
        CUSTOMS => by_zones(p, CUSTOMS_ZONES, 1),
        STREETS => by_zones(p, STREETS_ZONES, 1),
        RESERVE => by_zones(p, RESERVE_ZONES, 1),
        // этажи tarkov.dev: подвал (туннели) ниже −1, 1 этаж до 3, 2 этаж до 6
        FACTORY => by_height(p.y, &[-1.0, 3.0, 6.0]),
        // 16 слоёв tarkov.dev, сведённые в 6 этажей (границы — их heightRange)
        ICEBREAKER => by_height(p.y, &[11.9, 18.92, 22.2, 25.3, 34.5]),
        _ => 0,
    }
}

/// Этажи Эпицентра — по высоте, как слои карты tarkov.dev (`extents` в maps.json).
fn ground_zero_floor(p: Pos) -> usize {
    let inside = |x0: f64, x1: f64, z0: f64, z1: f64| (x0..=x1).contains(&p.x) && (z0..=z1).contains(&p.z);
    if p.y >= 32.3 {
        3
    } else if (28.0..32.3).contains(&p.y) || ((26.0..31.0).contains(&p.y) && inside(91.0, 98.0, 216.0, 228.0)) {
        // второй этаж; в шоуруме M он ниже
        2
    } else if p.y < 21.0 && (inside(43.0, 117.0, -100.0, 190.0) || inside(117.0, 143.0, 49.0, 80.0)) {
        // подземный гараж и переход
        0
    } else {
        1
    }
}

/// Этажи Развязки: улица с подземной парковкой ТЦ (y < 25), этажи ТЦ «УЛЬТРА» — 25–34 и выше 34.
/// Вне ТЦ всё — улица (эстакады тоже).
fn interchange_floor(p: Pos) -> usize {
    let in_mall = (-222.0..=120.0).contains(&p.x) && (-327.0..=218.0).contains(&p.z);
    match p.y {
        y if in_mall && y >= 34.0 => 2,
        y if in_mall && y >= 25.0 => 1,
        _ => 0,
    }
}

/// Этажи Берега — только у санатория, он выше всего на карте: y ≥ 2 — 3 этаж, −1…2 — 2 этаж;
/// подвал — ниже −5 под западным крылом и административным корпусом.
fn shoreline_floor(p: Pos) -> usize {
    let inside = |x0: f64, x1: f64, z0: f64, z1: f64| (x0..=x1).contains(&p.x) && (z0..=z1).contains(&p.z);
    if p.y >= 2.0 {
        3
    } else if p.y >= -1.0 {
        2
    } else if p.y < -5.0 && (inside(-237.0, -137.0, -104.0, -68.0) || inside(-268.0, -234.0, -163.0, -134.0)) {
        0
    } else {
        1
    }
}

/// Этаж по высоте: номер первой границы, которая выше точки (границы по возрастанию).
fn by_height(y: f64, bounds: &[f64]) -> usize {
    bounds.iter().position(|&b| y < b).unwrap_or(bounds.len())
}

/// Зона этажа: высота [от, до) и прямоугольники [x1, z1, x2, z2] (как `extents` в maps.json tarkov.dev).
struct Zone {
    floor: usize,
    y: (f64, f64),
    rects: &'static [[f64; 4]],
}

/// Этаж по первой подходящей зоне; иначе `default`.
fn by_zones(p: Pos, zones: &[Zone], default: usize) -> usize {
    zones
        .iter()
        .find(|z| {
            (z.y.0..z.y.1).contains(&p.y)
                && z.rects.iter().any(|r| {
                    (r[0].min(r[2])..=r[0].max(r[2])).contains(&p.x) && (r[1].min(r[3])..=r[1].max(r[3])).contains(&p.z)
                })
        })
        .map(|z| z.floor)
        .unwrap_or(default)
}

/// Таможня: подвалы (ЗБ-шки, котельная, старая заправка, подвал «Свитча»), 2 этаж, 3 этаж и выше
/// (4 этаж нефтевышки — тоже сюда). Порядок важен: у общаг зоны 2 и 3 этажа перекрываются.
const CUSTOMS_ZONES: &[Zone] = &[
    Zone {
        floor: 0,
        y: (-1000.0, 0.5),
        rects: &[
            [635.0, -137.0, 620.0, -125.0],  // ЗБ-1011
            [473.0, -122.0, 458.0, -110.0],  // ЗБ-1012
            [314.0, -173.0, 308.0, -184.0],  // старая заправка
            [349.0, -88.0, 323.0, -32.0],    // подвал «Свитча»
            [219.0, -158.0, 193.0, -137.0],  // ЗБ-013
            [122.0, -61.0, 88.0, -40.0],     // котельная
        ],
    },
    Zone { floor: 3, y: (5.7, 1000.0), rects: &[[243.0, 190.0, 165.0, 125.0]] }, // общаги
    Zone { floor: 3, y: (7.7, 11.3), rects: &[[73.0, -73.0, 22.0, -38.0]] },    // склад 17
    Zone { floor: 3, y: (6.7, 11.6), rects: &[[126.0, -64.0, 88.0, -35.0]] },   // котельная
    Zone { floor: 3, y: (8.0, 54.7), rects: &[[279.0, -79.0, 246.0, -1.4]] },   // нефтевышка
    Zone {
        floor: 2,
        y: (2.7, 6.5),
        rects: &[
            [243.0, 190.0, 165.0, 125.0],    // общаги
            [116.0, -83.0, 72.0, -170.0],    // Механик
            [356.0, -30.0, 341.0, -84.0],    // «Свитч»
            [334.0, -52.0, 321.0, -59.0],
            [589.0, 10.0, 577.0, -1.0],      // блокпост Диких
            [580.0, -104.0, 532.0, -134.0],  // склад с мёртвым Диким
            [625.0, -120.0, 599.0, -139.0],  // насосная
        ],
    },
    Zone {
        floor: 2,
        y: (5.7, 1000.0),
        rects: &[
            [580.0, -104.0, 532.0, -134.0],
            [-199.0, -90.0, -223.0, -131.0], // большой красный
            [239.0, 3.0, 169.0, -160.0],     // «Скелет»
            [336.0, -56.0, 316.0, -95.0],    // снайпер у «Свитча»
            [584.0, -46.0, 556.0, -92.0],    // USEC
            [93.0, 0.0, 65.0, -22.0],        // стройка
        ],
    },
    Zone { floor: 2, y: (14.0, 15.0), rects: &[[497.0, -44.0, 450.0, -90.0]] }, // химсклад, снайпер
    Zone { floor: 2, y: (3.9, 7.6), rects: &[[73.0, 57.0, 22.0, -38.0]] },     // склад 17
    Zone { floor: 2, y: (4.4, 6.5), rects: &[[119.0, -57.0, 100.0, -42.0]] },  // котельная
    Zone { floor: 2, y: (4.6, 7.9), rects: &[[279.0, -79.0, 246.0, -1.4]] },   // нефтевышка
];

/// Вся карта — для зон, которые зависят только от высоты.
const EVERYWHERE: &[[f64; 4]] = &[[-1e4, -1e4, 1e4, 1e4]];

/// Улицы: подземка ниже −6 (у центральной парковки — ниже 0, у «Конкордии» — ниже 2),
/// этажи — по 5 м от 10 (в бухгалтерии 2 этаж ниже — 5–10).
const STREETS_ZONES: &[Zone] = &[
    Zone { floor: 0, y: (-1e4, -6.0), rects: EVERYWHERE },
    Zone { floor: 0, y: (-1e4, 0.0), rects: &[[-13.0, -16.0, 28.0, 33.0]] },   // центральная парковка
    Zone { floor: 0, y: (-1e4, 2.0), rects: &[[151.0, 344.0, 275.0, 418.0]] }, // «Конкордия»
    Zone { floor: 4, y: (20.0, 1e4), rects: EVERYWHERE },
    Zone { floor: 3, y: (15.0, 20.0), rects: EVERYWHERE },
    Zone { floor: 2, y: (10.0, 15.0), rects: EVERYWHERE },
    Zone { floor: 2, y: (5.0, 10.0), rects: &[[-190.0, 222.0, -162.0, 237.5]] }, // бухгалтерия
];

const RESERVE_DOME: &[[f64; 4]] = &[[1.0, 164.0, -17.0, 199.0]];
const RESERVE_PAWNS: &[[f64; 4]] = &[[-77.0, 26.0, -177.0, 106.0]];
const RESERVE_BLACK_BISHOP: &[[f64; 4]] = &[[-104.0, -37.0, -177.0, 5.0]];
const RESERVE_WHITE_BISHOP: &[[f64; 4]] = &[[-47.0, -47.0, -85.0, -18.0]];
const RESERVE_WHITE_KING: &[[f64; 4]] = &[[-19.91, -13.0, -78.0, 39.0]];
const RESERVE_KNIGHTS: &[[f64; 4]] = &[[99.0, -50.0, -2.0, 7.0]];

/// Резерв: бункеры и этажи зданий («Пешки», «Слоны», «Король», «Кони», купол, депо, вышки).
/// У каждого здания свои высоты этажей — из `extents` tarkov.dev; 5 этаж «Пешек» — к 4-му.
const RESERVE_ZONES: &[Zone] = &[
    Zone {
        floor: 0,
        y: (-1e4, -7.27),
        rects: &[[128.0, -208.0, 18.0, -33.0], [-46.0, -42.0, -176.0, 127.0]], // склад, командные бункеры
    },
    Zone { floor: 0, y: (-1e4, -12.0), rects: &[[-40.0, 124.0, -124.0, 189.0]] }, // D-2
    Zone { floor: 0, y: (-1e4, 18.0), rects: &[[23.0, 173.0, -65.0, 189.0]] },   // туннели под куполом
    Zone { floor: 0, y: (-7.27, -3.2), rects: &[[74.0, -196.0, 19.0, -149.0]] }, // за гермодверью
    Zone {
        floor: 0,
        y: (-11.0, -4.6),
        rects: &[[-246.0, -79.0, -274.0, -53.0], [238.0, -26.0, 126.0, 45.0]], // у выхода E1, бункеры
    },
    Zone { floor: 4, y: (5.0, 9.5), rects: RESERVE_PAWNS },
    Zone { floor: 4, y: (29.3, 36.0), rects: RESERVE_DOME },
    Zone { floor: 4, y: (2.23, 5.0), rects: RESERVE_PAWNS },
    Zone { floor: 4, y: (2.15, 6.6), rects: RESERVE_WHITE_KING },
    Zone { floor: 4, y: (1.6, 4.7), rects: RESERVE_KNIGHTS },
    Zone { floor: 3, y: (25.7, 29.3), rects: RESERVE_DOME },
    Zone { floor: 3, y: (-0.64, 2.23), rects: RESERVE_PAWNS },
    Zone { floor: 3, y: (-0.64, 2.23), rects: RESERVE_BLACK_BISHOP },
    Zone { floor: 3, y: (-0.6, 10.0), rects: RESERVE_WHITE_BISHOP },
    Zone { floor: 3, y: (-2.2, 2.14), rects: RESERVE_WHITE_KING },
    Zone { floor: 3, y: (-1.1, 1.6), rects: RESERVE_KNIGHTS },
    Zone { floor: 2, y: (22.1, 25.7), rects: RESERVE_DOME },
    Zone { floor: 2, y: (-3.5, -0.64), rects: RESERVE_PAWNS },
    Zone { floor: 2, y: (-3.5, -0.64), rects: &[[62.0, 59.0, 51.0, 108.0]] }, // вышка у КПП
    Zone { floor: 2, y: (-3.5, -0.64), rects: RESERVE_BLACK_BISHOP },
    Zone { floor: 2, y: (-3.9, -0.6), rects: RESERVE_WHITE_BISHOP },
    Zone { floor: 2, y: (-4.3, -2.2), rects: RESERVE_WHITE_KING },
    Zone { floor: 2, y: (-3.8, -1.1), rects: RESERVE_KNIGHTS },
    Zone { floor: 2, y: (-1.9, 11.3), rects: &[[191.0, -175.0, 137.0, -120.0]] }, // депо
    Zone {
        floor: 2,
        y: (1.0, 8.0),
        rects: &[
            [-109.0, -156.0, -119.0, -147.0], // вышки
            [289.0, -92.0, 299.0, -82.0],
            [3.0, -210.0, -7.0, -200.0],
            [195.0, -260.0, 185.0, -250.0],
            [276.0, 17.0, 266.0, 27.0],
        ],
    },
    Zone { floor: 2, y: (-4.1, -1.2), rects: &[[-128.0, -139.0, -146.0, -120.0]] }, // земли Диких
];

/// SVG-подложка tarkov.dev и её проекция (`src/data/maps.json` в репозитории tarkov-dev).
#[derive(Clone, Copy, Debug)]
pub struct SvgMap {
    pub url: &'static str,
    /// transform [a, b, c, d]: Leaflet-точка = (a·rx + b, −c·ry + d)
    pub transform: [f64; 4],
    /// coordinateRotation, градусы
    pub rotation: f64,
    /// Углы картинки в игровых [x, z]: bounds из maps.json.
    pub bounds: [[f64; 2]; 2],
    /// Пикселей картинки на метр.
    pub px_per_m: f64,
    /// Слои SVG — группы `<g id="…">`, которые зависят от этажа.
    pub layers: &'static [&'static str],
    /// Какие из `layers` видны на каждом этаже.
    pub floors: &'static [&'static [&'static str]],
}

pub fn svg_map(map: &str) -> Option<SvgMap> {
    match map {
        LIGHTHOUSE => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/Lighthouse.svg",
            transform: [0.2, 0.0, 0.2, 0.0],
            rotation: 180.0,
            bounds: [[515.0, -998.0], [-545.0, 725.0]],
            px_per_m: 3.0,
            layers: &[],
            floors: &[&[]],
        }),
        // слой First_Floor tarkov.dev показывает всегда вместе с улицей
        GROUND_ZERO => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/GroundZero.svg",
            transform: [0.524, 167.3, 0.524, 65.1],
            rotation: 180.0,
            bounds: [[249.0, -124.0], [-99.0, 364.0]],
            px_per_m: 6.0,
            layers: &["Ground_Level", "Underground_Level", "First_Floor", "Second_Floor", "Third_Floor"],
            floors: &[
                &["Ground_Level", "First_Floor", "Underground_Level"],
                &["Ground_Level", "First_Floor"],
                &["Ground_Level", "First_Floor", "Second_Floor"],
                &["Ground_Level", "First_Floor", "Third_Floor"],
            ],
        }),
        // слои tarkov.dev: First_Floor — 1 этаж ТЦ (магазины), Second_Floor — 2 этаж ТЦ
        INTERCHANGE => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/Interchange.svg",
            transform: [0.265, 150.6, 0.265, 134.6],
            rotation: 180.0,
            bounds: [[598.0, -442.0], [-433.0, 426.0]],
            px_per_m: 3.0,
            layers: &["First_Floor", "Second_Floor"],
            floors: &[&[], &["First_Floor"], &["First_Floor", "Second_Floor"]],
        }),
        CUSTOMS => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/Customs.svg",
            transform: [0.239, 168.65, 0.239, 136.35],
            rotation: 180.0,
            bounds: [[698.0, -307.0], [-372.0, 237.0]],
            px_per_m: 3.0,
            layers: &["Underground_Level", "First_Floor", "Second_Floor", "Third_Floor"],
            floors: &[
                &["First_Floor", "Underground_Level"],
                &["First_Floor"],
                &["First_Floor", "Second_Floor"],
                &["First_Floor", "Third_Floor"],
            ],
        }),
        // карта большая — 2,5 px/м, чтобы четыре этажа не заняли лишнюю память
        SHORELINE => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/Shoreline.svg",
            transform: [0.16, 83.2, 0.16, 111.1],
            rotation: 180.0,
            bounds: [[504.0, -415.0], [-1056.0, 618.0]],
            px_per_m: 2.5,
            layers: &["Underground_Level", "First_Floor", "Second_Floor", "Third_Floor"],
            floors: &[
                &["First_Floor", "Underground_Level"],
                &["First_Floor"],
                &["First_Floor", "Second_Floor"],
                &["First_Floor", "Third_Floor"],
            ],
        }),
        // одноэтажная; большая — 2,5 px/м
        WOODS => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/Woods.svg",
            transform: [0.1855, 112.95, 0.1855, 167.85],
            rotation: 180.0,
            bounds: [[646.0, -914.0], [-761.0, 442.0]],
            px_per_m: 2.5,
            layers: &[],
            floors: &[&[]],
        }),
        // повёрнут на 90°; подвал в SVG лежит под 1 этажом — на его этаже 1 этаж прячем
        FACTORY => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/Factory.svg",
            transform: [1.629, 119.9, 1.629, 139.3],
            rotation: 90.0,
            bounds: [[77.0, -64.5], [-65.5, 67.4]],
            px_per_m: 8.0,
            layers: &["Basement", "Ground_Floor", "Second_Floor", "Third_Floor"],
            floors: &[
                &["Basement"],
                &["Ground_Floor"],
                &["Ground_Floor", "Second_Floor"],
                &["Ground_Floor", "Third_Floor"],
            ],
        }),
        // First_Floor — часть улицы; в подземке прячем, иначе он её закрывает
        STREETS => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/StreetsOfTarkov.svg",
            transform: [0.38, 0.0, 0.38, 0.0],
            rotation: 180.0,
            bounds: [[323.0, -295.0], [-280.0, 532.0]],
            px_per_m: 3.0,
            layers: &["Underground_Level", "First_Floor", "Second_Floor", "Third_Floor", "Fourth_Floor", "Fifth_Floor"],
            floors: &[
                &["Underground_Level"],
                &["First_Floor"],
                &["First_Floor", "Second_Floor"],
                &["First_Floor", "Third_Floor"],
                &["First_Floor", "Fourth_Floor", "Fifth_Floor"],
            ],
        }),
        _ => None,
    }
}

/// Подложка из тайлов tarkov.dev (`{z}/{x}/{y}.png`, сетка 2^z): Лаба, Резерв (этажи зданий
/// есть только тайлами), Лабиринт, Ледокол. Этаж — один или несколько слоёв, наложенных по порядку.
#[derive(Clone, Copy, Debug)]
pub struct TileMap {
    /// Префикс картинок в кэше.
    pub key: &'static str,
    /// Папка слоёв: `{root}/{слой}/{z}/{x}/{y}.png`.
    pub root: &'static str,
    pub transform: [f64; 4],
    pub rotation: f64,
    /// Размер тайла в точках Leaflet (tileSize tarkov.dev, по умолчанию 256).
    pub tile_size: f64,
    /// Зум, тайлы которого склеиваем.
    pub zoom: u32,
    /// Углы карты в игровых [x, z] — склеиваем всю; без них — только вокруг лута.
    pub bounds: Option<[[f64; 2]; 2]>,
    pub floors: &'static [&'static [&'static str]],
}

impl TileMap {
    /// Игровые (x, z) → доли сетки тайлов зума 0.
    pub fn uv(&self, x: f64, z: f64) -> (f64, f64) {
        let (lx, ly) = to_leaflet(self.rotation, self.transform, x, z);
        (lx / self.tile_size, ly / self.tile_size)
    }

    /// Доли сетки → игровые (x, z).
    pub fn game(&self, u: f64, v: f64) -> (f64, f64) {
        from_leaflet(self.rotation, self.transform, u * self.tile_size, v * self.tile_size)
    }

    /// Пикселей склеенной картинки на метр.
    pub fn px_per_m(&self) -> f64 {
        self.transform[0] * (256u32 << self.zoom) as f64 / self.tile_size
    }
}

pub fn tile_map(map: &str) -> Option<TileMap> {
    match map {
        LAB => Some(TileMap {
            key: "labs",
            root: "https://assets.tarkov.dev/maps/labs_v4",
            transform: [0.575, 281.2, 0.575, 193.7],
            rotation: 270.0,
            tile_size: 175.0,
            zoom: 4,
            bounds: None,
            floors: &[&["technical"], &["1st"], &["2nd"]],
        }),
        RESERVE => Some(TileMap {
            key: "reserve",
            root: "https://assets.tarkov.dev/maps/reserve",
            transform: [0.395, 122.0, 0.395, 137.65],
            rotation: 180.0,
            tile_size: 256.0,
            zoom: 3,
            bounds: Some([[289.0, -293.0], [-303.0, 244.0]]),
            floors: &[&["main", "bunkers"], &["main"], &["main", "2nd"], &["main", "3rd"], &["main", "4th", "5th"]],
        }),
        LABYRINTH => Some(TileMap {
            key: "labyrinth",
            root: "https://assets.tarkov.dev/maps/labyrinth",
            transform: [2.115, 85.5, 2.115, 128.0],
            rotation: 270.0,
            tile_size: 256.0,
            zoom: 3,
            bounds: Some([[-52.0, -37.0], [53.0, 76.0]]),
            floors: &[&["main"]],
        }),
        // transform tarkov.dev ([2, 125, 3.5, 91]) с картинкой не сходится — свой, подобран так,
        // чтобы лут и спавны легли на палубы своих слоёв (94 % точек против 31 %)
        ICEBREAKER => Some(TileMap {
            key: "icebreaker",
            root: "https://assets.tarkov.dev/maps/icebreaker",
            transform: [1.5, 126.5, 1.5, 127.5],
            rotation: 180.0,
            tile_size: 256.0,
            zoom: 4,
            // весь корабль (bounds tarkov.dev — от Завода)
            bounds: Some([[19.0, -80.0], [-21.0, 88.0]]),
            floors: &[
                &["00_control_room", "01_engine_room", "02_engine_room_upper", "03_fuel_pumps_lower"],
                &["04_fuel_pumps", "05_storage_ecurity"],
                &["06_infirmary", "07_helipad"],
                &["08_gym-canteen"],
                &["09_accommodation_lower", "10_accommodation_mid", "11_accommodation_upper"],
                &["12_officers_deck", "13_stairs_blocked", "14_bridge", "15_bridge_roof"],
            ],
        }),
        _ => None,
    }
}

/// Игровые (x, z) → точка Leaflet, как на tarkov.dev: поворот, затем transform.
fn to_leaflet(rotation: f64, transform: [f64; 4], x: f64, z: f64) -> (f64, f64) {
    let (s, c) = rotation.to_radians().sin_cos();
    let (rx, ry) = (x * c - z * s, x * s + z * c);
    let [a, b, cc, d] = transform;
    (a * rx + b, -cc * ry + d)
}

/// Точка Leaflet → игровые (x, z).
fn from_leaflet(rotation: f64, transform: [f64; 4], lx: f64, ly: f64) -> (f64, f64) {
    let [a, b, cc, d] = transform;
    let (rx, ry) = ((lx - b) / a, (d - ly) / cc);
    let (s, c) = rotation.to_radians().sin_cos();
    // поворот на −θ
    (rx * c + ry * s, -rx * s + ry * c)
}

impl SvgMap {
    /// Игровые (x, z) → точка Leaflet, как на tarkov.dev: поворот, затем transform.
    pub fn leaflet(&self, x: f64, z: f64) -> (f64, f64) {
        to_leaflet(self.rotation, self.transform, x, z)
    }

    /// Прямоугольник картинки в точках Leaflet: (левый, верхний, правый, нижний).
    pub fn leaflet_rect(&self) -> (f64, f64, f64, f64) {
        let p = self.leaflet(self.bounds[0][0], self.bounds[0][1]);
        let q = self.leaflet(self.bounds[1][0], self.bounds[1][1]);
        (p.0.min(q.0), p.1.min(q.1), p.0.max(q.0), p.1.max(q.1))
    }

    /// Размер картинки в пикселях (с учётом поворота: у Завода по горизонтали — z).
    pub fn image_size(&self) -> (u32, u32) {
        let (l, t, r, b) = self.leaflet_rect();
        let w = (r - l) / self.transform[0] * self.px_per_m;
        let h = (b - t) / self.transform[2] * self.px_per_m;
        (w.round() as u32, h.round() as u32)
    }

    /// Обратная проекция: точка Leaflet → игровые (x, z).
    pub fn game(&self, lx: f64, ly: f64) -> (f64, f64) {
        from_leaflet(self.rotation, self.transform, lx, ly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighthouse_projection() {
        let m = svg_map(LIGHTHOUSE).unwrap();
        let (l, t, r, b) = m.leaflet_rect();
        let (x, z) = (12.5, -340.0);
        let (lx, ly) = m.leaflet(x, z);
        assert!((l..=r).contains(&lx) && (t..=b).contains(&ly));
        let (gx, gz) = m.game(lx, ly);
        assert!((gx - x).abs() < 1e-9 && (gz - z).abs() < 1e-9);
        // поворот 180°: x растёт влево, z — вниз; левый верхний угол — bounds[0]
        let (lx0, ly0) = m.leaflet(515.0, -998.0);
        assert!((lx0 - l).abs() < 1e-9 && (ly0 - t).abs() < 1e-9);
        assert_eq!(m.image_size(), (3180, 5169));
    }

    #[test]
    fn ground_zero_floors() {
        let m = svg_map(GROUND_ZERO).unwrap();
        assert_eq!(m.floors.len(), floor_titles(GROUND_ZERO).len());
        assert_eq!(m.image_size(), (2088, 2928));
        // гараж под Тарбанком, улица, второй и третий этажи
        assert_eq!(floor_of(GROUND_ZERO, Pos { x: 107.0, y: 13.5, z: 50.6 }), 0);
        assert_eq!(floor_of(GROUND_ZERO, Pos { x: 156.2, y: 25.5, z: -83.6 }), 1);
        assert_eq!(floor_of(GROUND_ZERO, Pos { x: 0.0, y: 30.0, z: 0.0 }), 2);
        assert_eq!(floor_of(GROUND_ZERO, Pos { x: 0.0, y: 34.0, z: 0.0 }), 3);
    }

    #[test]
    fn interchange_floors() {
        let m = svg_map(INTERCHANGE).unwrap();
        assert_eq!(m.floors.len(), floor_titles(INTERCHANGE).len());
        assert_eq!(m.image_size(), (3093, 2604));
        assert_eq!(floor_of(INTERCHANGE, Pos { x: -50.0, y: 22.0, z: 0.0 }), 0);
        assert_eq!(floor_of(INTERCHANGE, Pos { x: -50.0, y: 29.0, z: 0.0 }), 1);
        assert_eq!(floor_of(INTERCHANGE, Pos { x: -50.0, y: 36.0, z: 0.0 }), 2);
        // вне ТЦ высоко — всё равно улица (эстакады)
        assert_eq!(floor_of(INTERCHANGE, Pos { x: 400.0, y: 36.0, z: 0.0 }), 0);
    }

    #[test]
    fn shoreline_floors() {
        let m = svg_map(SHORELINE).unwrap();
        assert_eq!(m.floors.len(), floor_titles(SHORELINE).len());
        assert_eq!(m.image_size(), (3900, 2583));
        assert_eq!(floor_of(SHORELINE, Pos { x: -200.0, y: -7.0, z: -80.0 }), 0);
        assert_eq!(floor_of(SHORELINE, Pos { x: -200.0, y: -3.0, z: -80.0 }), 1);
        assert_eq!(floor_of(SHORELINE, Pos { x: -200.0, y: 0.5, z: -80.0 }), 2);
        assert_eq!(floor_of(SHORELINE, Pos { x: -200.0, y: 3.0, z: -80.0 }), 3);
        // низина вне санатория — улица
        assert_eq!(floor_of(SHORELINE, Pos { x: 300.0, y: -58.0, z: 300.0 }), 1);
    }

    #[test]
    fn customs_floors() {
        let m = svg_map(CUSTOMS).unwrap();
        assert_eq!(m.floors.len(), floor_titles(CUSTOMS).len());
        assert_eq!(m.image_size(), (3210, 1632));
        // общаги: 1, 2 и 3 этажи
        assert_eq!(floor_of(CUSTOMS, Pos { x: 200.0, y: 1.0, z: 150.0 }), 1);
        assert_eq!(floor_of(CUSTOMS, Pos { x: 200.0, y: 4.0, z: 150.0 }), 2);
        assert_eq!(floor_of(CUSTOMS, Pos { x: 200.0, y: 7.0, z: 150.0 }), 3);
        // подвал котельной и чистое поле
        assert_eq!(floor_of(CUSTOMS, Pos { x: 100.0, y: -2.0, z: -50.0 }), 0);
        assert_eq!(floor_of(CUSTOMS, Pos { x: -300.0, y: 8.0, z: 0.0 }), 1);
    }

    #[test]
    fn lab_tiles_projection() {
        // прежняя формула Лабы: u = (0.575·z + 281.2)/175, v = (0.575·x + 193.7)/175
        let m = tile_map(LAB).unwrap();
        let (x, z) = (-150.0, -330.0);
        let (u, v) = m.uv(x, z);
        assert!((u - (0.575 * z + 281.2) / 175.0).abs() < 1e-12);
        assert!((v - (0.575 * x + 193.7) / 175.0).abs() < 1e-12);
        let (gx, gz) = m.game(u, v);
        assert!((gx - x).abs() < 1e-9 && (gz - z).abs() < 1e-9);
    }

    #[test]
    fn every_available_map_has_backdrop() {
        for m in MAPS.iter().filter(|m| m.available) {
            let floors = svg_map(m.id).map(|s| s.floors.len()).or_else(|| tile_map(m.id).map(|t| t.floors.len()));
            assert_eq!(floors, Some(floor_titles(m.id).len()), "{}", m.id);
            assert_eq!(floor_tabs(m.id).len(), floor_titles(m.id).len(), "{}", m.id);
        }
    }

    #[test]
    fn factory_rotated_size() {
        // повёрнут на 90°: по горизонтали — z (131,9 м), по вертикали — x (142,5 м), как у SVG
        assert_eq!(svg_map(FACTORY).unwrap().image_size(), (1055, 1140));
        assert_eq!(floor_of(FACTORY, Pos { x: 0.0, y: -2.0, z: 0.0 }), 0);
        assert_eq!(floor_of(FACTORY, Pos { x: 0.0, y: 0.5, z: 0.0 }), 1);
        assert_eq!(floor_of(FACTORY, Pos { x: 0.0, y: 4.0, z: 0.0 }), 2);
        assert_eq!(floor_of(FACTORY, Pos { x: 0.0, y: 7.0, z: 0.0 }), 3);
    }

    #[test]
    fn streets_and_reserve_floors() {
        assert_eq!(floor_of(STREETS, Pos { x: 0.0, y: -8.0, z: 0.0 }), 0);
        assert_eq!(floor_of(STREETS, Pos { x: 0.0, y: -2.0, z: 0.0 }), 0); // центральная парковка
        assert_eq!(floor_of(STREETS, Pos { x: 100.0, y: -2.0, z: 100.0 }), 1);
        assert_eq!(floor_of(STREETS, Pos { x: 100.0, y: 12.0, z: 100.0 }), 2);
        assert_eq!(floor_of(STREETS, Pos { x: 100.0, y: 22.0, z: 100.0 }), 4);
        // «Пешки»: 2, 3, 4 и 5 этажи; рядом на той же высоте — улица
        assert_eq!(floor_of(RESERVE, Pos { x: -120.0, y: -2.0, z: 60.0 }), 2);
        assert_eq!(floor_of(RESERVE, Pos { x: -120.0, y: 1.0, z: 60.0 }), 3);
        assert_eq!(floor_of(RESERVE, Pos { x: -120.0, y: 3.0, z: 60.0 }), 4);
        assert_eq!(floor_of(RESERVE, Pos { x: -120.0, y: 7.0, z: 60.0 }), 4);
        assert_eq!(floor_of(RESERVE, Pos { x: 200.0, y: -2.0, z: 200.0 }), 1);
        assert_eq!(floor_of(RESERVE, Pos { x: -100.0, y: -9.0, z: 0.0 }), 0); // командные бункеры
    }
}
