//! Склейка тайлов tarkov.dev в одну картинку на этаж (кэшируется на диске).

use anyhow::Result;
use image::{ImageFormat, RgbaImage};
use std::path::PathBuf;

use crate::data::{data_dir, http_get, LabData};
use crate::labs::{self, Room, FLOORS, TILE_ZOOM};

/// Какой кусок сетки тайлов склеен в картинки (одинаково для всех этажей).
#[derive(Clone, Copy, Debug)]
pub struct TileRange {
    pub tx0: u32,
    pub ty0: u32,
    pub w: u32,
    pub h: u32,
}

impl TileRange {
    /// Охватывает все точки лута и двери с запасом.
    pub fn for_data(data: &LabData) -> Self {
        let n = (1u32 << TILE_ZOOM) as f64;
        let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        let positions = data.loot.iter().map(|l| l.pos).chain(data.locks.iter().map(|l| l.pos));
        for p in positions {
            let (u, v) = labs::grid_uv(p.x, p.z);
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(v);
            v1 = v1.max(v);
        }
        let margin = 0.03;
        let tx0 = ((u0 - margin) * n).floor().max(0.0) as u32;
        let ty0 = ((v0 - margin) * n).floor().max(0.0) as u32;
        let tx1 = ((u1 + margin) * n).ceil().min(n) as u32;
        let ty1 = ((v1 + margin) * n).ceil().min(n) as u32;
        TileRange { tx0, ty0, w: tx1 - tx0, h: ty1 - ty0 }
    }

    pub fn pixel_size(&self) -> (f32, f32) {
        ((self.w * 256) as f32, (self.h * 256) as f32)
    }

    /// Игровые координаты -> пиксели склеенной картинки этажа.
    pub fn project(&self, x: f64, z: f64) -> (f32, f32) {
        let (u, v) = labs::grid_uv(x, z);
        let full = (256u32 << TILE_ZOOM) as f64;
        ((u * full - (self.tx0 * 256) as f64) as f32, (v * full - (self.ty0 * 256) as f64) as f32)
    }

    /// Пиксели склеенной картинки -> игровые (x, z).
    pub fn unproject(&self, px: f32, py: f32) -> (f64, f64) {
        let full = (256u32 << TILE_ZOOM) as f64;
        let u = (px as f64 + (self.tx0 * 256) as f64) / full;
        let v = (py as f64 + (self.ty0 * 256) as f64) / full;
        ((v * 175.0 - 193.7) / 0.575, (u * 175.0 - 281.2) / 0.575)
    }

    pub fn floor_path(&self, floor: usize) -> PathBuf {
        data_dir().join("maps").join(format!(
            "labs_{}_z{}_{}_{}_{}x{}.png",
            FLOORS[floor].tile_dir, TILE_ZOOM, self.tx0, self.ty0, self.w, self.h
        ))
    }
}

/// Скачивает и склеивает недостающие этажи.
pub fn ensure_floors(range: TileRange) -> Result<()> {
    std::fs::create_dir_all(data_dir().join("maps"))?;
    for floor in 0..FLOORS.len() {
        let path = range.floor_path(floor);
        if path.exists() {
            continue;
        }
        let img = stitch(range, FLOORS[floor].tile_dir)?;
        let tmp = path.with_extension("png.tmp");
        img.save_with_format(&tmp, ImageFormat::Png)?;
        std::fs::rename(tmp, path)?;
    }
    Ok(())
}

fn stitch(range: TileRange, dir: &str) -> Result<RgbaImage> {
    let mut out = RgbaImage::from_pixel(range.w * 256, range.h * 256, image::Rgba([0, 0, 0, 0]));
    let coords: Vec<(u32, u32)> = (0..range.w)
        .flat_map(|x| (0..range.h).map(move |y| (range.tx0 + x, range.ty0 + y)))
        .collect();
    let tiles: Vec<((u32, u32), Option<RgbaImage>)> = std::thread::scope(|s| {
        let handles: Vec<_> = coords
            .chunks(coords.len().div_ceil(8).max(1))
            .map(|chunk| {
                s.spawn(move || {
                    chunk
                        .iter()
                        .map(|&(x, y)| {
                            let url = format!("{}/{dir}/{TILE_ZOOM}/{x}/{y}.png", labs::TILE_ROOT);
                            let img = http_get(&url)
                                .ok()
                                .and_then(|b| image::load_from_memory_with_format(&b, ImageFormat::Png).ok())
                                .map(|i| i.to_rgba8());
                            ((x, y), img)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    for ((x, y), tile) in tiles {
        if let Some(tile) = tile {
            image::imageops::replace(
                &mut out,
                &tile,
                ((x - range.tx0) * 256) as i64,
                ((y - range.ty0) * 256) as i64,
            );
        }
    }
    Ok(out)
}

/// Картинка этажа с дорисованными комнатами, которых нет на подложке tarkov.dev
/// (`solid` в labs_rooms.json). Кэшируется по хэшу таких комнат.
pub fn floor_image(range: TileRange, floor: usize, rooms: &[Room]) -> Result<PathBuf> {
    use std::hash::{Hash, Hasher};
    let base = range.floor_path(floor);
    let solid: Vec<&Room> = rooms.iter().filter(|r| r.solid && r.floor == floor).collect();
    if solid.is_empty() {
        return Ok(base);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in &solid {
        for p in r.poly.iter().chain(r.texture_from.iter()) {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
        }
    }
    let stem = base.file_stem().unwrap().to_string_lossy().to_string();
    let path = base.with_file_name(format!("{stem}_p{:x}.png", h.finish()));
    if path.exists() {
        return Ok(path);
    }

    let mut img = image::open(&base)?.to_rgba8();
    let (w, hgt) = (img.width() as i64, img.height() as i64);
    const WALL: f32 = 3.0;
    // плитка пола ~1 м
    let tile = (256u32 << TILE_ZOOM) as f32 * 0.575 / 175.0;
    for room in solid {
        let pts: Vec<(f32, f32)> = room.poly.iter().map(|p| range.project(p[0], p[1])).collect();
        // ровный пол цвета соседнего помещения
        let (dx, dy) = room
            .texture_from
            .map(|t| range.project(t[0], t[1]))
            .unwrap_or((pts[0].0 + 40.0, pts[0].1));
        let mut acc = [0u64; 3];
        let mut n = 0u64;
        for y in (dy as i64 - 12)..(dy as i64 + 12) {
            for x in (dx as i64 - 12)..(dx as i64 + 12) {
                if x >= 0 && y >= 0 && x < w && y < hgt {
                    let p = img.get_pixel(x as u32, y as u32);
                    if p[3] > 200 {
                        for c in 0..3 {
                            acc[c] += p[c] as u64;
                        }
                        n += 1;
                    }
                }
            }
        }
        let floor = if n > 0 {
            [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8]
        } else {
            [58, 60, 63]
        };
        let grout = floor.map(|c| c.saturating_sub(10));
        let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
        let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
        for y in (y0.floor() as i64).max(0)..(y1.ceil() as i64).min(hgt) {
            for x in (x0.floor() as i64).max(0)..(x1.ceil() as i64).min(w) {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                if !pip(&pts, fx, fy) {
                    continue;
                }
                let c = if edge_dist(&pts, fx, fy) < WALL {
                    [24, 24, 24]
                } else if (fx - x0).rem_euclid(tile) < 1.0 || (fy - y0).rem_euclid(tile) < 1.0 {
                    grout
                } else {
                    floor
                };
                img.put_pixel(x as u32, y as u32, image::Rgba([c[0], c[1], c[2], 255]));
            }
        }
    }
    let tmp = path.with_extension("png.tmp");
    img.save_with_format(&tmp, ImageFormat::Png)?;
    std::fs::rename(tmp, &path)?;
    Ok(path)
}

fn pip(pts: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn edge_dist(pts: &[(f32, f32)], x: f32, y: f32) -> f32 {
    let mut best = f32::MAX;
    for i in 0..pts.len() {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % pts.len()];
        let (vx, vy) = (bx - ax, by - ay);
        let t = (((x - ax) * vx + (y - ay) * vy) / (vx * vx + vy * vy).max(1e-6)).clamp(0.0, 1.0);
        let (cx, cy) = (ax + t * vx - x, ay + t * vy - y);
        best = best.min((cx * cx + cy * cy).sqrt());
    }
    best
}
