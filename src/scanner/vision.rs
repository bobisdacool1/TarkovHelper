//! Поиск предметов на снимке инвентаря: сетка клеток → занятые клетки → квадраты предметов.
//! Только работа с картинкой, без платформенного кода — проверяется тестами на эталонном скрине
//! `tools/scanner-prototype/backpack.png` (алгоритм и пороги — из прототипа, см. его README).

use image::GrayImage;

/// Сетка клеток: вертикальные линии `x0 + k*cell`, горизонтальные `y0 + k*cell`.
#[derive(Clone, Copy, Debug)]
pub struct Lattice {
    pub x0: f32,
    pub y0: f32,
    pub cell: f32,
}

impl Lattice {
    fn col(&self, x: f32) -> i32 {
        ((x - self.x0) / self.cell).floor() as i32
    }
    fn row(&self, y: f32) -> i32 {
        ((y - self.y0) / self.cell).floor() as i32
    }
    fn x(&self, c: i32) -> f32 {
        self.x0 + c as f32 * self.cell
    }
    fn y(&self, r: i32) -> f32 {
        self.y0 + r as f32 * self.cell
    }
}

/// Квадрат предмета на снимке: пиксели и размер в клетках.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemBox {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    pub w: u32,
    pub h: u32,
    /// Клетки сложились в прямоугольник. Нет — склейка ошиблась, размеру не верим.
    pub exact: bool,
}

impl ItemBox {
    /// Полоска ярлыка: короткое имя написано в правом верхнем углу, выровнено по правому краю.
    /// Берём не больше трёх правых клеток — левее у длинных предметов только картинка.
    pub fn label_rect(&self, cell: f32) -> (u32, u32, u32, u32) {
        let x1 = self.x1.saturating_sub(2);
        let x0 = (self.x0 + 2).max(x1.saturating_sub((cell * 3.0) as u32));
        let y0 = self.y0 + 1;
        let y1 = (self.y0 + (cell * 0.27).round() as u32).min(self.y1);
        (x0, y0, x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1))
    }
}

/// Размер клетки по разрешению экрана: 63 px в 1920×1080, интерфейс масштабируется по меньшей стороне.
pub fn cell_hint(screen_w: u32, screen_h: u32) -> f32 {
    63.0 * (screen_w as f32 / 1920.0).min(screen_h as f32 / 1080.0)
}

#[inline]
fn px(g: &GrayImage, x: i32, y: i32) -> i32 {
    let x = x.clamp(0, g.width() as i32 - 1) as u32;
    let y = y.clamp(0, g.height() as i32 - 1) as u32;
    g.get_pixel(x, y).0[0] as i32
}

/// Насколько вертикальная линия в x светлее фона по бокам (на расстоянии d).
#[inline]
fn vline(g: &GrayImage, x: i32, y: i32, d: i32) -> i32 {
    let c = px(g, x - 1, y).max(px(g, x, y)).max(px(g, x + 1, y));
    c - (px(g, x - d, y) + px(g, x + d, y)) / 2
}

#[inline]
fn hline(g: &GrayImage, x: i32, y: i32, d: i32) -> i32 {
    let c = px(g, x, y - 1).max(px(g, x, y)).max(px(g, x, y + 1));
    c - (px(g, x, y - d) + px(g, x, y + d)) / 2
}

/// Лучшая фаза сетки с шагом `s` по профилю линий: (средний отклик на линиях, фаза).
fn best_phase(profile: &[f32], start: i32, s: f32) -> (f32, f32) {
    let mut best = (f32::MIN, 0.0);
    let len = profile.len() as i32;
    let mut phase = 0.0;
    while phase < s {
        let mut k = ((start as f32 - phase) / s).ceil() as i32;
        let (mut sum, mut n) = (0.0, 0);
        loop {
            let p = (phase + k as f32 * s).round() as i32 - start;
            if p >= len {
                break;
            }
            if p >= 0 {
                sum += profile[p as usize];
                n += 1;
            }
            k += 1;
        }
        if n >= 3 {
            let avg = sum / n as f32;
            if avg > best.0 {
                best = (avg, phase);
            }
        }
        phase += 0.5;
    }
    best
}

/// Ищет сетку клеток вокруг точки: шаг около `hint`, фазы по x и y.
/// Линии сетки — и тонкие светлые полосы между пустыми клетками, и рамки предметов —
/// лежат на одной решётке, а картинки предметов случайны и усредняются.
pub fn find_lattice(g: &GrayImage, cx: u32, cy: u32, hint: f32) -> Option<Lattice> {
    let (w, h) = (g.width() as i32, g.height() as i32);
    let d = ((hint * 0.048).round() as i32).max(3);
    let span = (hint * 4.0) as i32;
    let avg = (hint * 2.5) as i32;
    let (cx, cy) = (cx as i32, cy as i32);

    let xs = ((cx - span).max(d + 1), (cx + span).min(w - d - 2));
    let ys = ((cy - span).max(d + 1), (cy + span).min(h - d - 2));
    if xs.1 - xs.0 < (hint * 2.0) as i32 || ys.1 - ys.0 < (hint * 2.0) as i32 {
        return None;
    }
    let rows = ((cy - avg).max(0), (cy + avg).min(h - 1));
    let cols = ((cx - avg).max(0), (cx + avg).min(w - 1));
    let pv: Vec<f32> = (xs.0..=xs.1)
        .map(|x| {
            let (mut s, mut n) = (0, 0);
            for y in (rows.0..=rows.1).step_by(2) {
                s += vline(g, x, y, d).clamp(0, 40);
                n += 1;
            }
            s as f32 / n.max(1) as f32
        })
        .collect();
    let ph: Vec<f32> = (ys.0..=ys.1)
        .map(|y| {
            let (mut s, mut n) = (0, 0);
            for x in (cols.0..=cols.1).step_by(2) {
                s += hline(g, x, y, d).clamp(0, 40);
                n += 1;
            }
            s as f32 / n.max(1) as f32
        })
        .collect();
    let mean = |p: &[f32]| p.iter().sum::<f32>() / p.len().max(1) as f32;
    let (mv, mh) = (mean(&pv), mean(&ph));

    let mut best: Option<(f32, Lattice)> = None;
    let mut s = hint * 0.85;
    while s <= hint * 1.15 {
        let (sx, px0) = best_phase(&pv, xs.0, s);
        let (sy, py0) = best_phase(&ph, ys.0, s);
        let score = sx + sy;
        if best.as_ref().is_none_or(|b| score > b.0) {
            best = Some((score, Lattice { x0: px0, y0: py0, cell: s }));
        }
        s += 0.1;
    }
    let (score, lat) = best?;
    // решётка должна заметно выделяться над средним откликом
    (score > 1.6 * (mv + mh) + 3.0).then_some(lat)
}

/// Клетки вокруг курсора: занятость, рамки между клетками, принадлежность сетке.
struct Cells<'a> {
    g: &'a GrayImage,
    lat: Lattice,
    d: i32,
}

impl Cells<'_> {
    fn inside(&self, c: i32, r: i32) -> bool {
        let m = self.d as f32 + 2.0;
        self.lat.x(c) >= m
            && self.lat.y(r) >= m
            && self.lat.x(c + 1) < self.g.width() as f32 - m
            && self.lat.y(r + 1) < self.g.height() as f32 - m
    }

    /// Занята ли клетка: пустые почти однотонные (σ≈2), с предметом σ≥14.
    fn occupied(&self, c: i32, r: i32) -> bool {
        let s = self.lat.cell;
        let m = s * 0.12;
        let step = (s / 20.0).max(1.0);
        let (mut sum, mut sum2, mut n) = (0.0f64, 0.0f64, 0.0f64);
        let mut y = self.lat.y(r) + m;
        while y < self.lat.y(r + 1) - m {
            let mut x = self.lat.x(c) + m;
            while x < self.lat.x(c + 1) - m {
                let v = px(self.g, x as i32, y as i32) as f64;
                sum += v;
                sum2 += v * v;
                n += 1.0;
                x += step;
            }
            y += step;
        }
        if n == 0.0 {
            return false;
        }
        let mean = sum / n;
        (sum2 / n - mean * mean).max(0.0).sqrt() > 8.0
    }

    /// Доля точек отрезка, где линия светлее фона больше чем на `thr`.
    /// Вертикальный отрезок: линия x = lat.x(c), строка r; горизонтальный — y = lat.y(r), столбец c.
    /// Фаза решётки известна с точностью до пары пикселей — берём лучший сдвиг.
    fn line_frac(&self, c: i32, r: i32, vertical: bool, thr: i32) -> f32 {
        let s = self.lat.cell;
        let trim = s * 0.14;
        let (from, to) = if vertical {
            (self.lat.y(r) + trim, self.lat.y(r + 1) - trim)
        } else {
            (self.lat.x(c) + trim, self.lat.x(c + 1) - trim)
        };
        let base = if vertical { self.lat.x(c) } else { self.lat.y(r) }.round() as i32;
        let mut best = 0.0f32;
        for off in -2..=2 {
            let line = base + off;
            let (mut hit, mut n) = (0, 0);
            let mut t = from;
            while t < to {
                let resp = if vertical {
                    vline(self.g, line, t as i32, self.d)
                } else {
                    hline(self.g, t as i32, line, self.d)
                };
                hit += (resp > thr) as i32;
                n += 1;
                t += 1.0;
            }
            if n > 0 {
                best = best.max(hit as f32 / n as f32);
            }
        }
        best
    }

    /// Доля точек границы, где фон по одну сторону ровно светлее (или темнее), чем по другую.
    /// У каждого предмета свой однотонный фон, а картинка внутри одного предмета через границу
    /// клеток идёт непрерывно — перепады случайные и разного знака.
    fn step_frac(&self, c: i32, r: i32, vertical: bool) -> f32 {
        let s = self.lat.cell;
        let trim = s * 0.14;
        let k = self.d + 2;
        let (from, to) = if vertical {
            (self.lat.y(r) + trim, self.lat.y(r + 1) - trim)
        } else {
            (self.lat.x(c) + trim, self.lat.x(c + 1) - trim)
        };
        let line = if vertical { self.lat.x(c) } else { self.lat.y(r) }.round() as i32;
        let mut diffs = Vec::new();
        let mut t = from;
        while t < to {
            let (a, b) = if vertical {
                (px(self.g, line - k, t as i32), px(self.g, line + k, t as i32))
            } else {
                (px(self.g, t as i32, line - k), px(self.g, t as i32, line + k))
            };
            diffs.push(b - a);
            t += 1.0;
        }
        if diffs.is_empty() {
            return 0.0;
        }
        // перепад двух однотонных фонов почти одинаков вдоль всей границы (6±1),
        // а градиент или картинка внутри предмета дают разброс
        let mut sorted = diffs.clone();
        sorted.sort();
        let m = sorted[sorted.len() / 2];
        if m.abs() < 3 {
            return 0.0;
        }
        diffs.iter().filter(|d| (**d - m).abs() <= 1).count() as f32 / diffs.len() as f32
    }

    /// Разные ли предметы в соседних занятых клетках: светлая рамка между ними
    /// (внутри одного предмета доля 0,03–0,23, между разными 0,83–1,00) или перепад фона.
    fn split(&self, c: i32, r: i32, vertical: bool) -> bool {
        self.line_frac(c, r, vertical, 10) >= 0.5 || self.step_frac(c, r, vertical) >= 0.6
    }
    fn split_right(&self, c: i32, r: i32) -> bool {
        self.split(c + 1, r, true)
    }
    fn split_down(&self, c: i32, r: i32) -> bool {
        self.split(c, r + 1, false)
    }
}

/// Склейка занятых клеток окна в предметы (union-find).
struct Groups {
    c0: i32,
    r0: i32,
    n: i32,
    parent: Vec<usize>,
    occ: Vec<bool>,
}

impl Groups {
    fn build(cells: &Cells, cc: i32, cr: i32, radius: i32) -> Self {
        let n = radius * 2 + 1;
        let (c0, r0) = (cc - radius, cr - radius);
        let mut g = Groups { c0, r0, n, parent: (0..(n * n) as usize).collect(), occ: vec![false; (n * n) as usize] };
        for r in 0..n {
            for c in 0..n {
                let (ac, ar) = (c0 + c, r0 + r);
                g.occ[(r * n + c) as usize] = cells.inside(ac, ar) && cells.occupied(ac, ar);
            }
        }
        for r in 0..n {
            for c in 0..n {
                if !g.occ[(r * n + c) as usize] {
                    continue;
                }
                let (ac, ar) = (c0 + c, r0 + r);
                if c + 1 < n && g.occ[(r * n + c + 1) as usize] && !cells.split_right(ac, ar) {
                    g.union((r * n + c) as usize, (r * n + c + 1) as usize);
                }
                if r + 1 < n && g.occ[((r + 1) * n + c) as usize] && !cells.split_down(ac, ar) {
                    g.union((r * n + c) as usize, ((r + 1) * n + c) as usize);
                }
            }
        }
        g
    }

    fn find(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.parent[b] = a;
        }
    }

    fn index(&self, c: i32, r: i32) -> Option<usize> {
        let (lc, lr) = (c - self.c0, r - self.r0);
        (lc >= 0 && lr >= 0 && lc < self.n && lr < self.n).then_some((lr * self.n + lc) as usize)
    }

    /// Клетки (абсолютные) группы, в которую входит клетка `i`.
    fn members(&mut self, i: usize) -> Vec<(i32, i32)> {
        let root = self.find(i);
        let mut out = Vec::new();
        for k in 0..self.parent.len() {
            if self.occ[k] && self.find(k) == root {
                out.push((self.c0 + k as i32 % self.n, self.r0 + k as i32 / self.n));
            }
        }
        out
    }
}

fn to_box(lat: &Lattice, cells: &[(i32, i32)]) -> ItemBox {
    let cmin = cells.iter().map(|c| c.0).min().unwrap_or(0);
    let cmax = cells.iter().map(|c| c.0).max().unwrap_or(0);
    let rmin = cells.iter().map(|c| c.1).min().unwrap_or(0);
    let rmax = cells.iter().map(|c| c.1).max().unwrap_or(0);
    let (w, h) = ((cmax - cmin + 1) as u32, (rmax - rmin + 1) as u32);
    ItemBox {
        x0: lat.x(cmin).round().max(0.0) as u32,
        y0: lat.y(rmin).round().max(0.0) as u32,
        x1: lat.x(cmax + 1).round().max(0.0) as u32,
        y1: lat.y(rmax + 1).round().max(0.0) as u32,
        w,
        h,
        exact: cells.len() as u32 == w * h,
    }
}

fn cells_for(g: &GrayImage, lat: Lattice) -> Cells<'_> {
    Cells { g, lat, d: ((lat.cell * 0.048).round() as i32).max(3) }
}

/// Предмет под курсором: сетка вокруг, клетка под курсором и склеенный с ней квадрат.
pub fn item_at(g: &GrayImage, cx: u32, cy: u32, hint: f32) -> Option<(ItemBox, Lattice)> {
    let lat = find_lattice(g, cx, cy, hint)?;
    let cells = cells_for(g, lat);
    let (cc, cr) = (lat.col(cx as f32), lat.row(cy as f32));
    if !cells.inside(cc, cr) || !cells.occupied(cc, cr) {
        return None;
    }
    let mut groups = Groups::build(&cells, cc, cr, 9);
    let i = groups.index(cc, cr)?;
    let b = to_box(&lat, &groups.members(i));
    (b.w <= 10 && b.h <= 10).then_some((b, lat))
}

/// Все предметы контейнера под курсором (рюкзак, разгрузка, карманы).
/// Контейнер — связная область «клеток сетки»: пустых клеток и предметов, по краям которых
/// идут линии решётки. Фон вокруг и соседние контейнеры с другой фазой сетки сюда не попадают.
pub fn items_around(g: &GrayImage, cx: u32, cy: u32, hint: f32) -> Option<(Vec<ItemBox>, Lattice)> {
    let lat = find_lattice(g, cx, cy, hint)?;
    let cells = cells_for(g, lat);
    let (cc, cr) = (lat.col(cx as f32), lat.row(cy as f32));
    const R: i32 = 12;
    let mut groups = Groups::build(&cells, cc, cr, R);

    // узел: группа занятых клеток (по корню) или пустая клетка
    let node_of = |groups: &mut Groups, c: i32, r: i32| -> Option<(bool, usize)> {
        let i = groups.index(c, r)?;
        if !cells.inside(c, r) {
            return None;
        }
        Some(if groups.occ[i] { (true, groups.find(i)) } else { (false, i) })
    };
    // доля края узла, вдоль которого идёт линия решётки
    let grid_like = |members: &[(i32, i32)]| -> bool {
        let set: std::collections::HashSet<(i32, i32)> = members.iter().copied().collect();
        let (mut sum, mut n) = (0.0, 0);
        for &(c, r) in members {
            if !set.contains(&(c - 1, r)) {
                sum += cells.line_frac(c, r, true, 5);
                n += 1;
            }
            if !set.contains(&(c + 1, r)) {
                sum += cells.line_frac(c + 1, r, true, 5);
                n += 1;
            }
            if !set.contains(&(c, r - 1)) {
                sum += cells.line_frac(c, r, false, 5);
                n += 1;
            }
            if !set.contains(&(c, r + 1)) {
                sum += cells.line_frac(c, r + 1, false, 5);
                n += 1;
            }
        }
        n > 0 && sum / n as f32 >= 0.6
    };

    let mut seen = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut items = Vec::new();
    let start = node_of(&mut groups, cc, cr)?;
    queue.push_back((start, cc, cr));
    seen.insert(start);
    while let Some(((occupied, id), c, r)) = queue.pop_front() {
        let members = if occupied { groups.members(id) } else { vec![(c, r)] };
        if !grid_like(&members) {
            continue;
        }
        if occupied {
            let b = to_box(&lat, &members);
            if b.w <= 10 && b.h <= 10 {
                items.push(b);
            }
        }
        for &(mc, mr) in &members {
            for (nc, nr) in [(mc - 1, mr), (mc + 1, mr), (mc, mr - 1), (mc, mr + 1)] {
                if let Some(node) = node_of(&mut groups, nc, nr) {
                    if seen.insert(node) {
                        queue.push_back((node, nc, nr));
                    }
                }
            }
        }
    }
    items.sort_by_key(|b| (b.y0, b.x0));
    Some((items, lat))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backpack() -> GrayImage {
        let bytes = include_bytes!("../../tools/scanner-prototype/backpack.png");
        image::load_from_memory(bytes).unwrap().to_luma8()
    }

    /// Скрин снят в 2560×1440: клетка 84 px.
    const HINT: f32 = 84.0;

    #[test]
    fn lattice_matches_prototype_frame() {
        let g = backpack();
        let lat = find_lattice(&g, 330, 260, HINT).expect("сетка не найдена");
        assert!((lat.cell - 84.4).abs() < 1.0, "клетка {}", lat.cell);
        // рамка сетки в прототипе: X0 = 205, Y0 = 58
        let fx = (lat.x0 - 205.0).rem_euclid(lat.cell);
        let fy = (lat.y0 - 58.0).rem_euclid(lat.cell);
        assert!(fx.min(lat.cell - fx) <= 3.0, "фаза x {}", lat.x0);
        assert!(fy.min(lat.cell - fy) <= 3.0, "фаза y {}", lat.y0);
    }

    #[test]
    fn item_under_cursor() {
        let g = backpack();
        // (точка, ожидаемый размер w×h)
        let cases = [
            ((250, 110), (2, 1)), // Sausage
            ((420, 100), (1, 1)), // Roler
            ((300, 270), (2, 1)), // SVDS
            ((415, 330), (1, 2)), // Salewa
            ((590, 100), (1, 2)), // Wedge — фигурка на две клетки
            ((590, 200), (1, 2)),
            ((330, 180), (1, 1)), // Wires
        ];
        for ((x, y), (w, h)) in cases {
            let (b, _) = item_at(&g, x, y, HINT).unwrap_or_else(|| panic!("нет предмета в ({x},{y})"));
            assert_eq!((b.w, b.h), (w, h), "размер в ({x},{y}): {b:?}");
            assert!(b.exact);
        }
        // пустая клетка
        assert!(item_at(&g, 300, 450, HINT).is_none());
    }

    #[test]
    fn whole_backpack() {
        let g = backpack();
        let (items, _) = items_around(&g, 300, 450, HINT).expect("сетка не найдена");
        // слот самого рюкзака слева (рамка с ярлыком Tri-Zip) тоже похож на предмет — его можно
        // снять галочкой в окне «После рейда»; здесь проверяем содержимое сетки (рамка с x = 205)
        let items: Vec<ItemBox> = items.into_iter().filter(|b| b.x0 >= 200).collect();
        let mut sizes: Vec<(u32, u32)> = items.iter().map(|b| (b.w, b.h)).collect();
        sizes.sort();
        // 12 предметов: Sausage и SVDS 2×1, Wedge и Salewa 1×2, восемь 1×1
        let mut expect = vec![(1, 1); 8];
        expect.extend([(1, 2), (1, 2), (2, 1), (2, 1)]);
        expect.sort();
        assert_eq!(sizes, expect, "{items:?}");
    }
}
