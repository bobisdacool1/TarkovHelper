//! Список карт и то, чем они различаются: этажи, подложка, проекция координат.

use crate::labs::{self, Pos};

pub const LAB: &str = "the-lab";
pub const LIGHTHOUSE: &str = "lighthouse";

pub struct MapInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub available: bool,
}

pub const MAPS: &[MapInfo] = &[
    MapInfo { id: LAB, name: "Лаборатория", available: true },
    MapInfo { id: "customs", name: "Таможня", available: false },
    MapInfo { id: "woods", name: "Лес", available: false },
    MapInfo { id: "shoreline", name: "Берег", available: false },
    MapInfo { id: "interchange", name: "Развязка", available: false },
    MapInfo { id: "reserve", name: "Резерв", available: false },
    MapInfo { id: LIGHTHOUSE, name: "Маяк", available: true },
    MapInfo { id: "streets-of-tarkov", name: "Улицы Таркова", available: false },
    MapInfo { id: "factory", name: "Завод", available: false },
    MapInfo { id: "ground-zero", name: "Эпицентр", available: false },
    MapInfo { id: "the-labyrinth", name: "Лабиринт", available: false },
    MapInfo { id: "icebreaker", name: "Ледокол", available: false },
];

/// Карта по id; неизвестная или недоступная — Лаба.
pub fn get(id: &str) -> &'static MapInfo {
    MAPS.iter().find(|m| m.id == id && m.available).unwrap_or(&MAPS[0])
}

pub fn name(id: &str) -> &'static str {
    MAPS.iter().find(|m| m.id == id).map(|m| m.name).unwrap_or("?")
}

/// Заголовки этажей слева направо. Одноэтажные карты — один безымянный этаж.
pub fn floor_titles(map: &str) -> &'static [&'static str] {
    match map {
        LAB => &["Технический этаж", "1 этаж", "2 этаж"],
        _ => &[""],
    }
}

/// Этаж точки.
pub fn floor_of(map: &str, p: Pos) -> usize {
    match map {
        LAB => labs::floor_of(p),
        _ => 0,
    }
}

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
}

pub fn svg_map(map: &str) -> Option<SvgMap> {
    match map {
        LIGHTHOUSE => Some(SvgMap {
            url: "https://assets.tarkov.dev/maps/svg/Lighthouse.svg",
            transform: [0.2, 0.0, 0.2, 0.0],
            rotation: 180.0,
            bounds: [[515.0, -998.0], [-545.0, 725.0]],
            px_per_m: 3.0,
        }),
        _ => None,
    }
}

impl SvgMap {
    /// Игровые (x, z) → точка Leaflet, как на tarkov.dev: поворот, затем transform.
    pub fn leaflet(&self, x: f64, z: f64) -> (f64, f64) {
        let (s, c) = self.rotation.to_radians().sin_cos();
        let (rx, ry) = (x * c - z * s, x * s + z * c);
        let [a, b, cc, d] = self.transform;
        (a * rx + b, -cc * ry + d)
    }

    /// Прямоугольник картинки в точках Leaflet: (левый, верхний, правый, нижний).
    pub fn leaflet_rect(&self) -> (f64, f64, f64, f64) {
        let p = self.leaflet(self.bounds[0][0], self.bounds[0][1]);
        let q = self.leaflet(self.bounds[1][0], self.bounds[1][1]);
        (p.0.min(q.0), p.1.min(q.1), p.0.max(q.0), p.1.max(q.1))
    }

    /// Размер картинки в пикселях.
    pub fn image_size(&self) -> (u32, u32) {
        let w = (self.bounds[0][0] - self.bounds[1][0]).abs() * self.px_per_m;
        let h = (self.bounds[0][1] - self.bounds[1][1]).abs() * self.px_per_m;
        (w.round() as u32, h.round() as u32)
    }

    /// Обратная проекция: точка Leaflet → игровые (x, z).
    pub fn game(&self, lx: f64, ly: f64) -> (f64, f64) {
        let [a, b, cc, d] = self.transform;
        let (rx, ry) = ((lx - b) / a, (d - ly) / cc);
        let (s, c) = self.rotation.to_radians().sin_cos();
        // поворот на −θ
        (rx * c + ry * s, -rx * s + ry * c)
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
}
