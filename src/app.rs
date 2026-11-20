//! Главное окно: меню в стиле mapgenie слева и карта Лабы (три этажа рядом).

use gpui::{
    canvas, div, img, point, prelude::*, svg, Hsla, PathBuilder, px, rgb, rgba, AnyElement, App, Bounds, ClickEvent, Context,
    ElementId, Entity, Focusable, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    Pixels, Point, Render, ScrollDelta, ScrollWheelEvent, SharedString, Window,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::categories::GROUPS;
use crate::data::{self, fmt_rub, fmt_short, Item, LabData, Mode};
use crate::labs::{key_color, point_in_poly, FLOORS};
use crate::settings;
use crate::text_input::{InputChanged, TextInput};
use crate::tiles::{self, TileRange};

const SIDEBAR_W: f32 = 400.0;
const PAD: f32 = 16.0;
const FLOOR_GAP: f32 = 160.0;
/// Ключи сворачиваемых секций меню.
const SECTIONS: [&str; 4] = ["price", "access", "loot", "loc"];

const THRESHOLDS: [i64; 7] = [0, 50_000, 100_000, 200_000, 300_000, 500_000, 1_000_000];

// палитра в духе mapgenie: тёмная панель, золото, кремовый текст
const BG: u32 = 0x111111;
const PANEL: u32 = 0x1c1b19;
const CARD: u32 = 0x262420;
const GOLD: u32 = 0xc2ab74;
const GOLD_DIM: u32 = 0x6f6248;
const TEXT: u32 = 0xebe3cf;
const MUTED: u32 = 0x7d7666;

enum UpdateState {
    None,
    Available(std::sync::Arc<cargo_packager_updater::Update>),
    Installing(String),
    Failed(String),
}

enum Status {
    Idle,
    Busy(String),
    Error(String),
}

pub struct HelperApp {
    mode: Mode,
    status: Status,
    data: Option<Rc<LabData>>,
    range: Option<TileRange>,
    /// Картинки этажей (с дорисованными комнатами).
    floor_images: Vec<std::path::PathBuf>,
    threshold: i64,
    search: String,
    search_input: Entity<TextInput>,
    hidden_items: HashSet<String>,
    hidden_groups: HashSet<usize>,
    /// Скрытый доступ: None — лут без ключа, Some(id) — за дверью с этим ключом.
    hidden_access: HashSet<Option<String>>,
    expanded: HashSet<usize>,
    /// Свёрнутые секции меню.
    collapsed: HashSet<&'static str>,
    /// None — все этажи рядом, Some(i) — один этаж на весь экран.
    floor_view: Option<usize>,
    show_doors: bool,
    show_rooms: bool,
    show_extracts: bool,
    /// Автообновление цен: при старте, если данные старше часа, и каждые 30 минут.
    auto_refresh: bool,
    /// Обновление приложения.
    update: UpdateState,
    /// Последнее записанное на диск состояние настроек.
    saved: settings::Settings,
    selected_point: Option<usize>,
    /// Наведённый объект карты — свой тултип без задержки.
    hover: Option<Hover>,
    // карта
    scale: f32,
    offset: (f32, f32),
    fitted: bool,
    drag: Option<(Point<Pixels>, (f32, f32))>,
    drag_moved: bool,
    map_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
}

struct Hover {
    key: String,
    lines: Vec<String>,
    floor: usize,
    x: f64,
    z: f64,
}

/// Точка лута, прошедшая фильтры.
struct Visible {
    idx: usize,
    best: i64,
    top_item: String,
    items: Vec<String>,
}

struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(GOLD_DIM))
            .px_2()
            .py_1()
            .text_sm()
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}

fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text: SharedString = text.into();
    move |_, cx| {
        let t = text.clone();
        cx.new(|_| Tip(t)).into()
    }
}

fn tier_color(price: i64) -> u32 {
    match price {
        p if p >= 1_000_000 => 0xff5252,
        p if p >= 500_000 => 0xffa726,
        p if p >= 200_000 => 0xba68c8,
        p if p >= 100_000 => 0x42a5f5,
        _ => 0x9e9e9e,
    }
}

/// Чёрный или белый текст поверх цвета.
fn contrast(color: u32) -> u32 {
    let (r, g, b) = ((color >> 16) & 0xff, (color >> 8) & 0xff, color & 0xff);
    if r * 299 + g * 587 + b * 114 > 150_000 { 0x111111 } else { 0xffffff }
}

fn with_alpha(color: u32, alpha: u32) -> gpui::Rgba {
    rgba((color << 8) | alpha)
}

impl HelperApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| TextInput::new("Поиск… (ledx, черная, -плакат)", cx));
        cx.subscribe(&search_input, |this, input, _: &InputChanged, cx| {
            this.search = input.read(cx).text().to_string();
            this.selected_point = None;
            cx.notify();
        })
        .detach();
        let st = settings::load();
        let mut this = Self {
            mode: if st.mode == "pvp" { Mode::Pvp } else { Mode::Pve },
            status: Status::Idle,
            data: None,
            range: None,
            floor_images: Vec::new(),
            threshold: st.threshold,
            search: String::new(),
            search_input: search_input.clone(),
            hidden_items: st.hidden_items.iter().cloned().collect(),
            hidden_groups: st.hidden_groups.iter().copied().collect(),
            hidden_access: st
                .hidden_access
                .iter()
                .map(|k| if k.is_empty() { None } else { Some(k.clone()) })
                .collect(),
            expanded: st.expanded.iter().copied().collect(),
            collapsed: SECTIONS.iter().copied().filter(|k| st.collapsed.contains(*k)).collect(),
            floor_view: st.floor_view,
            show_doors: st.show_doors,
            show_rooms: st.show_rooms,
            show_extracts: st.show_extracts,
            auto_refresh: st.auto_refresh,
            saved: st,
            update: UpdateState::None,
            selected_point: None,
            hover: None,
            scale: 0.3,
            offset: (20.0, 40.0),
            fitted: false,
            drag: None,
            drag_moved: false,
            map_bounds: Rc::new(RefCell::new(None)),
        };
        this.load(false, cx);
        // новая версия приложения?
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async { crate::updater::check() }).await;
            if let Ok(Some(update)) = found {
                this.update(cx, |this, cx| {
                    this.update = UpdateState::Available(std::sync::Arc::new(update));
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        // фоновое автообновление цен
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(30 * 60)).await;
            let alive = this.update(cx, |this, cx| {
                if this.auto_refresh && !matches!(this.status, Status::Busy(_)) {
                    this.load(true, cx);
                }
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();
        this
    }

    fn install_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(update) = &self.update else { return };
        let update = update.clone();
        self.update = UpdateState::Installing(update.version.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            // качаем, проверяем подпись и ставим поверх текущей версии
            let result = cx
                .background_executor()
                .spawn(async move { update.download_and_install() })
                .await;
            this.update(cx, |this, cx| match result {
                Ok(()) => cx.restart(),
                Err(e) => {
                    this.update = UpdateState::Failed(format!("{e:#}"));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn to_settings(&self) -> settings::Settings {
        settings::Settings {
            mode: if self.mode == Mode::Pvp { "pvp" } else { "pve" }.into(),
            threshold: self.threshold,
            hidden_items: self.hidden_items.iter().cloned().collect(),
            hidden_groups: self.hidden_groups.iter().copied().collect(),
            hidden_access: self.hidden_access.iter().map(|k| k.clone().unwrap_or_default()).collect(),
            expanded: self.expanded.iter().copied().collect(),
            collapsed: self.collapsed.iter().map(|k| k.to_string()).collect(),
            floor_view: self.floor_view,
            show_doors: self.show_doors,
            show_rooms: self.show_rooms,
            show_extracts: self.show_extracts,
            auto_refresh: self.auto_refresh,
        }
    }

    /// Пишет настройки на диск, только если что-то поменялось.
    fn persist(&mut self) {
        let now = self.to_settings();
        if now != self.saved {
            settings::save(&now);
            self.saved = now;
        }
    }

    /// Загружает данные режима (из кэша или скачивает дамп) и готовит подложку.
    fn load(&mut self, force_download: bool, cx: &mut Context<Self>) {
        let mode = self.mode;
        let need_download = force_download || !data::has_dump(mode);
        self.status = Status::Busy(if need_download {
            format!("Скачиваю дамп {}…", mode.label())
        } else {
            "Загрузка…".into()
        });
        cx.notify();
        let started = std::time::Instant::now();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if need_download {
                        data::download_dump(mode)?;
                        crate::labs::refresh_rooms_from_repo();
                    }
                    let lab = data::load_lab(mode)?;
                    let range = TileRange::for_data(&lab);
                    tiles::ensure_floors(range)?;
                    let images = (0..FLOORS.len())
                        .map(|f| tiles::floor_image(range, f, &lab.rooms))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    anyhow::Ok((lab, range, images))
                })
                .await;
            this.update(cx, |this, cx| {
                let mut refresh_after = false;
                match result {
                    Ok((lab, range, images)) if lab.mode == this.mode => {
                        eprintln!(
                            "tarkov-helper: данные {} ({}) за {} мс",
                            mode.label(),
                            if need_download { "скачаны" } else { "из кэша" },
                            started.elapsed().as_millis()
                        );
                        let stale = lab.fetched_at.is_none_or(|t| {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(t);
                            now.saturating_sub(t) > 3600
                        });
                        // показали кэш, а свежие цены докачаем следом
                        refresh_after = !need_download && stale && this.auto_refresh;
                        this.data = Some(Rc::new(lab));
                        this.range = Some(range);
                        this.floor_images = images;
                        this.status = Status::Idle;
                        this.selected_point = None;
                    }
                    Ok(_) => {}
                    Err(e) => this.status = Status::Error(format!("{e:#}")),
                }
                if refresh_after {
                    this.load(true, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            self.load(false, cx);
        }
    }

    // ---------- фильтры ----------

    /// Проходит порог цены и поиск (без учёта категорий и ручного скрытия).
    fn matches_base(&self, item: &Item) -> bool {
        if item.price() < self.threshold {
            return false;
        }
        // слова через пробел должны все совпасть; "-слово" исключает
        let q = data::normalize(self.search.trim());
        q.split_whitespace().all(|w| match w.strip_prefix('-') {
            Some("") => true,
            Some(neg) => !item.search.contains(neg),
            None => item.search.contains(w),
        })
    }

    fn is_shown(&self, item: &Item) -> bool {
        self.matches_base(item)
            && !self.hidden_groups.contains(&item.group)
            && !self.hidden_items.contains(&item.id)
    }

    fn access_of(&self, d: &LabData, point: usize) -> Option<String> {
        d.loot[point].room.map(|r| d.rooms[r].key.clone())
    }

    /// Комната под курсором (локальные координаты карты).
    fn room_at(&self, d: &LabData, local: (f32, f32)) -> Option<usize> {
        let range = self.range?;
        d.rooms.iter().position(|room| {
            if !self.floor_visible(room.floor) {
                return false;
            }
            let wx = (local.0 - self.offset.0) / self.scale - self.floor_origin(room.floor);
            let wy = (local.1 - self.offset.1) / self.scale;
            let (x, z) = range.unproject(wx, wy);
            point_in_poly(&room.poly, x, z)
        })
    }

    fn visible_points(&self, d: &LabData) -> Vec<Visible> {
        d.loot
            .iter()
            .enumerate()
            .filter_map(|(idx, p)| {
                if self.hidden_access.contains(&self.access_of(d, idx)) {
                    return None;
                }
                let mut items: Vec<String> = p
                    .items
                    .iter()
                    .filter(|id| d.items.get(*id).is_some_and(|i| self.is_shown(i)))
                    .cloned()
                    .collect();
                if items.is_empty() {
                    return None;
                }
                items.sort_by(|a, b| d.items[b].price().cmp(&d.items[a].price()).then(a.cmp(b)));
                items.dedup();
                let best = d.items[&items[0]].price();
                Some(Visible { idx, best, top_item: items[0].clone(), items })
            })
            .collect()
    }

    // ---------- карта ----------

    fn hover_listener(
        key: String,
        lines: Vec<String>,
        floor: usize,
        x: f64,
        z: f64,
        cx: &mut Context<Self>,
    ) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered {
                this.hover = Some(Hover { key: key.clone(), lines: lines.clone(), floor, x, z });
            } else if this.hover.as_ref().is_some_and(|h| h.key == key) {
                this.hover = None;
            } else {
                return;
            }
            cx.notify();
        })
    }

    fn floor_visible(&self, floor: usize) -> bool {
        self.floor_view.is_none_or(|f| f == floor)
    }

    fn world_size(&self) -> Option<(f32, f32)> {
        let (w, h) = self.range?.pixel_size();
        let n = if self.floor_view.is_some() { 1 } else { FLOORS.len() };
        Some((w * n as f32 + FLOOR_GAP * (n - 1) as f32, h))
    }

    fn floor_origin(&self, floor: usize) -> f32 {
        if self.floor_view.is_some() {
            return 0.0;
        }
        let (w, _) = self.range.map(|r| r.pixel_size()).unwrap_or((0.0, 0.0));
        floor as f32 * (w + FLOOR_GAP)
    }

    fn set_floor_view(&mut self, view: Option<usize>, cx: &mut Context<Self>) {
        self.floor_view = view;
        self.hover = None;
        self.fit();
        cx.notify();
    }

    fn to_screen(&self, floor: usize, x: f64, z: f64) -> (f32, f32) {
        let (px_, py_) = self.range.unwrap().project(x, z);
        (
            self.offset.0 + (self.floor_origin(floor) + px_) * self.scale,
            self.offset.1 + py_ * self.scale,
        )
    }

    fn fit(&mut self) {
        let bounds = *self.map_bounds.borrow();
        let (Some(b), Some((ww, wh))) = (bounds, self.world_size()) else {
            return;
        };
        let (vw, vh) = (f32::from(b.size.width), f32::from(b.size.height) - 30.0);
        self.scale = (vw / ww).min(vh / wh) * 0.97;
        self.offset = ((vw - ww * self.scale) / 2.0, 30.0 + (vh - wh * self.scale) / 2.0);
        self.fitted = true;
    }

    fn zoom_at(&mut self, factor: f32, local: (f32, f32)) {
        let new = (self.scale * factor).clamp(0.05, 4.0);
        let f = new / self.scale;
        self.offset.0 = local.0 - (local.0 - self.offset.0) * f;
        self.offset.1 = local.1 - (local.1 - self.offset.1) * f;
        self.scale = new;
    }

    fn zoom_center(&mut self, factor: f32) {
        let bounds = *self.map_bounds.borrow();
        if let Some(b) = bounds {
            let c = (f32::from(b.size.width) / 2.0, f32::from(b.size.height) / 2.0);
            self.zoom_at(factor, c);
        }
    }

    fn render_map(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.fitted && self.map_bounds.borrow().is_some() && self.range.is_some() {
            self.fit();
        }
        let bounds_cell = self.map_bounds.clone();
        let mut layers: Vec<AnyElement> = Vec::new();

        layers.push(
            canvas(
                move |bounds, window, _| {
                    let first = bounds_cell.borrow().is_none();
                    *bounds_cell.borrow_mut() = Some(bounds);
                    if first {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
            .into_any_element(),
        );

        if let (Some(d), Some(range)) = (self.data.clone(), self.range) {
            let (iw, ih) = range.pixel_size();
            let s = self.scale;
            for f in 0..FLOORS.len() {
                if !self.floor_visible(f) {
                    continue;
                }
                let x = self.offset.0 + self.floor_origin(f) * s;
                let y = self.offset.1;
                layers.push(
                    img(self.floor_images.get(f).cloned().unwrap_or_else(|| range.floor_path(f)))
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(iw * s))
                        .h(px(ih * s))
                        .into_any_element(),
                );
                layers.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y - 26.0))
                        .text_color(rgb(GOLD))
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .child(FLOORS[f].title.to_uppercase())
                        .into_any_element(),
                );
            }

            if self.show_rooms {
                let hovered_room = self
                    .hover
                    .as_ref()
                    .and_then(|h| h.key.strip_prefix("room").and_then(|n| n.parse::<usize>().ok()));
                let polys: Vec<(Vec<Point<Pixels>>, u32, bool, bool)> = d
                    .rooms
                    .iter()
                    .enumerate()
                    .filter(|(_, room)| self.floor_visible(room.floor))
                    .map(|(ri, room)| {
                        let pts = room
                            .poly
                            .iter()
                            .map(|p| {
                                let (x, y) = self.to_screen(room.floor, p[0], p[1]);
                                point(px(x), px(y))
                            })
                            .collect();
                        (pts, key_color(&room.key), hovered_room == Some(ri), room.solid)
                    })
                    .collect();
                layers.push(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            for (pts, color, hot, _solid) in &polys {
                                if pts.len() < 3 {
                                    continue;
                                }
                                let o = bounds.origin;
                                let mut fill = PathBuilder::fill();
                                let mut line = PathBuilder::stroke(px(if *hot { 2.5 } else { 1.5 }));
                                fill.move_to(pts[0] + o);
                                line.move_to(pts[0] + o);
                                for p in &pts[1..] {
                                    fill.line_to(*p + o);
                                    line.line_to(*p + o);
                                }
                                fill.close();
                                line.close();
                                if let Ok(path) = fill.build() {
                                    window.paint_path(path, Hsla::from(with_alpha(*color, if *hot { 0x55 } else { 0x26 })));
                                }
                                if let Ok(path) = line.build() {
                                        window.paint_path(path, Hsla::from(with_alpha(if *color == 0x212121 { 0xbdbdbd } else { *color }, 0xe6)));
                                }
                            }
                        },
                    )
                    .absolute()
                    .size_full()
                    .into_any_element(),
                );
                if s > 0.3 {
                    for room in d.rooms.iter().filter(|r| self.floor_visible(r.floor)) {
                        let (cxm, czm) = room.center();
                        let (sx, sy) = self.to_screen(room.floor, cxm, czm);
                        layers.push(
                            div()
                                .absolute()
                                .left(px(sx - 60.0))
                                .top(px(sy - 8.0))
                                .w(px(120.0))
                                .flex()
                                .justify_center()
                                .child(
                                    div()
                                        .px_1()
                                        .bg(with_alpha(0x000000, 0x99))
                                        .text_xs()
                                        .text_color(rgb(TEXT))
                                        .child(room.name.clone()),
                                )
                                .into_any_element(),
                        );
                    }
                }
            }

            if self.show_doors {
                for (li, lock) in d.locks.iter().enumerate().filter(|(_, l)| self.floor_visible(l.floor)) {
                    let (sx, sy) = self.to_screen(lock.floor, lock.pos.x, lock.pos.z);
                    let color = key_color(&lock.key);
                    let fg = contrast(color);
                    let (short, lines) = match d.items.get(&lock.key) {
                        Some(k) => (
                            k.short.clone(),
                            vec![format!("Заперто: {}", k.name), format!("Цена ключа: {}", fmt_rub(k.price()))],
                        ),
                        None => (String::from("?"), vec![format!("Заперто: {}", lock.key)]),
                    };
                    let dark = color == 0x212121;
                    let label = s > 0.22 || dark;
                    layers.push(
                        div()
                            .id(ElementId::named_usize("lock", li))
                            .absolute()
                            .left(px(sx - 10.0))
                            .top(px(sy - 10.0))
                            .h(px(20.0))
                            .flex()
                            .items_center()
                            .gap_1()
                            .px(px(4.0))
                            .rounded_md()
                            .bg(rgb(color))
                            .border_1()
                            .border_color(rgb(if dark { 0xe0e0e0 } else { 0x111111 }))
                            .when(dark, |el| el.border_2())
                            .shadow_md()
                            .child(svg().flex_none().path("icons/lock.svg").size(px(12.0)).text_color(rgb(fg)))
                            .when(label, |el| {
                                el.child(div().text_xs().font_weight(FontWeight::BOLD).text_color(rgb(fg)).child(short))
                            })
                            .on_hover(Self::hover_listener(format!("lock{li}"), lines, lock.floor, lock.pos.x, lock.pos.z, cx))
                            .into_any_element(),
                    );
                }
            }

            if self.show_extracts {
                for (ei, ex) in d.extracts.iter().enumerate().filter(|(_, e)| self.floor_visible(e.floor)) {
                    let (sx, sy) = self.to_screen(ex.floor, ex.pos.x, ex.pos.z);
                    let (color, icon, kind) = match ex.faction.as_str() {
                        "pmc" => (0x43a047, "icons/exit.svg", "Выход ЧВК"),
                        "scav" => (0xfb8c00, "icons/exit.svg", "Выход Дикого"),
                        "transit" => (0x7e57c2, "icons/transit.svg", "Переход"),
                        _ => (0x26a69a, "icons/exit.svg", "Общий выход"),
                    };
                    layers.push(
                        div()
                            .id(ElementId::named_usize("exit", ei))
                            .absolute()
                            .left(px(sx - 12.0))
                            .top(px(sy - 12.0))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .gap_1()
                            .px(px(5.0))
                            .rounded_md()
                            .bg(rgb(color))
                            .border_1()
                            .border_color(rgb(0xffffff))
                            .shadow_md()
                            .child(svg().flex_none().path(icon).size(px(14.0)).text_color(rgb(0xffffff)))
                            .when(s > 0.15, |el| {
                                el.child(div().text_xs().font_weight(FontWeight::BOLD).text_color(rgb(0xffffff)).child(ex.name.clone()))
                            })
                            .on_hover(Self::hover_listener(
                                format!("exit{ei}"),
                                vec![kind.to_string(), ex.name.clone(), FLOORS[ex.floor].title.to_string()],
                                ex.floor,
                                ex.pos.x,
                                ex.pos.z,
                                cx,
                            ))
                            .into_any_element(),
                    );
                }
            }

            for v in self.visible_points(&d) {
                let p = &d.loot[v.idx];
                if !self.floor_visible(p.floor) {
                    continue;
                }
                let (sx, sy) = self.to_screen(p.floor, p.pos.x, p.pos.z);
                let selected = self.selected_point == Some(v.idx);
                let top = &d.items[&v.top_item];
                let mut lines: Vec<String> = v
                    .items
                    .iter()
                    .map(|id| {
                        let it = &d.items[id];
                        format!("{} — {}", it.short, fmt_short(it.price()))
                    })
                    .collect();
                if let Some(r) = p.room {
                    lines.push(format!("🔒 нужна {}", d.item_short(&d.rooms[r].key)));
                }
                let idx = v.idx;
                let size = if selected { 26.0 } else { 20.0 };
                layers.push(
                    div()
                        .id(ElementId::named_usize("pt", idx))
                        .absolute()
                        .left(px(sx - size / 2.0))
                        .top(px(sy - size / 2.0))
                        .size(px(size))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgb(tier_color(v.best)))
                        .border_2()
                        .border_color(rgb(if selected { 0xffffff } else { 0x161616 }))
                        .shadow_md()
                        .text_size(px(size * 0.55))
                        .child(GROUPS[top.group].icon)
                        .cursor_pointer()
                        .on_hover(Self::hover_listener(format!("pt{idx}"), lines, p.floor, p.pos.x, p.pos.z, cx))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.selected_point = Some(idx);
                            this.hover = None;
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                if s > 0.6 {
                    layers.push(
                        div()
                            .absolute()
                            .left(px(sx + size / 2.0 + 2.0))
                            .top(px(sy - 9.0))
                            .px_1()
                            .bg(with_alpha(0x000000, 0xb0))
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .child(format!("{} {}", top.short, fmt_short(top.price())))
                            .into_any_element(),
                    );
                }
            }

            if let Some(h) = &self.hover {
                let (sx, sy) = self.to_screen(h.floor, h.x, h.z);
                layers.push(
                    div()
                        .absolute()
                        .left(px(sx + 16.0))
                        .top(px(sy + 12.0))
                        .flex()
                        .flex_col()
                        .px_2()
                        .py_1()
                        .bg(rgb(PANEL))
                        .border_1()
                        .border_color(rgb(GOLD_DIM))
                        .shadow_lg()
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .children(h.lines.iter().map(|l| div().child(l.clone())))
                        .into_any_element(),
                );
            }

            if let Some(pi) = self.selected_point {
                let p = &d.loot[pi];
                let (sx, sy) = self.to_screen(p.floor, p.pos.x, p.pos.z);
                layers.push(self.render_popup(&d, pi, sx + 18.0, sy - 20.0, cx).into_any_element());
            }
        }

        // переключатель этажей
        let views: [(Option<usize>, &'static str); 4] =
            [(None, "Все этажи"), (Some(0), "Подвал"), (Some(1), "1 этаж"), (Some(2), "2 этаж")];
        layers.push(
            div()
                .absolute()
                .top(px(10.0))
                .left(px(10.0))
                .flex()
                .bg(rgb(PANEL))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(views.into_iter().enumerate().map(|(i, (view, label))| {
                    segmented(
                        ElementId::named_usize("floor-view", i),
                        label,
                        self.floor_view == view,
                        cx.listener(move |this, _, _, cx| {
                            this.set_floor_view(view, cx);
                            cx.stop_propagation();
                        }),
                    )
                }))
                .into_any_element(),
        );

        // кнопки зума
        layers.push(
            div()
                .absolute()
                .top(px(10.0))
                .right(px(10.0))
                .flex()
                .flex_col()
                .gap_1()
                .child(map_button("zin", "+", cx.listener(|this, _, _, cx| {
                    this.zoom_center(1.4);
                    cx.notify();
                })))
                .child(map_button("zout", "−", cx.listener(|this, _, _, cx| {
                    this.zoom_center(1.0 / 1.4);
                    cx.notify();
                })))
                .child(map_button("zfit", "⤢", cx.listener(|this, _, _, cx| {
                    this.fit();
                    cx.notify();
                })))
                .into_any_element(),
        );

        div()
            .id("map")
            .relative()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .bg(rgb(BG))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, _| {
                    this.drag = Some((ev.position, this.offset));
                    this.drag_moved = false;
                    window.blur();
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if let Some((start, off)) = this.drag {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        this.drag = None;
                        return;
                    }
                    let (dx, dy) = (f32::from(ev.position.x - start.x), f32::from(ev.position.y - start.y));
                    if dx.abs() + dy.abs() > 3.0 {
                        this.drag_moved = true;
                    }
                    this.offset = (off.0 + dx, off.1 + dy);
                    cx.notify();
                    return;
                }
                // наведение на комнату (маркеры важнее — их hover не трогаем)
                if !this.show_rooms || this.hover.as_ref().is_some_and(|h| !h.key.starts_with("room")) {
                    return;
                }
                let bounds = *this.map_bounds.borrow();
                let (Some(d), Some(b)) = (this.data.clone(), bounds) else { return };
                let local = (f32::from(ev.position.x - b.origin.x), f32::from(ev.position.y - b.origin.y));
                let new = this.room_at(&d, local).map(|ri| {
                    let room = &d.rooms[ri];
                    let (x, z) = room.center();
                    let mut lines = vec![format!("Комната: {}", room.name), format!("Нужна: {}", d.item_short(&room.key))];
                    if room.approx {
                        lines.push("граница примерная".into());
                    }
                    Hover { key: format!("room{ri}"), lines, floor: room.floor, x, z }
                });
                if new.as_ref().map(|h| &h.key) != this.hover.as_ref().map(|h| &h.key) {
                    this.hover = new;
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.drag = None))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, _| this.drag = None))
            // клик по пустому месту карты снимает выделение
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                if !this.drag_moved && this.selected_point.is_some() {
                    this.selected_point = None;
                    cx.notify();
                }
            }))
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _, cx| {
                let bounds = *this.map_bounds.borrow();
                let Some(b) = bounds else { return };
                let local = (
                    f32::from(ev.position.x - b.origin.x),
                    f32::from(ev.position.y - b.origin.y),
                );
                // Как на mapgenie: любая прокрутка (колесо, тачпад) — зум в точку под курсором,
                // перемещение — перетаскиванием.
                let factor = match ev.delta {
                    ScrollDelta::Lines(l) => 1.2f32.powf(l.y),
                    ScrollDelta::Pixels(p) => (f32::from(p.y) * 0.004).exp(),
                };
                this.zoom_at(factor.clamp(0.5, 2.0), local);
                cx.notify();
            }))
            .children(layers)
    }

    /// Карточка выбранной точки рядом с маркером.
    fn render_popup(&self, d: &LabData, pi: usize, x: f32, y: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let p = &d.loot[pi];
        let mut items: Vec<&Item> = p.items.iter().filter_map(|id| d.items.get(id)).collect();
        items.sort_by_key(|i| -i.price());
        items.dedup_by_key(|i| i.id.clone());

        let mut card = div()
            .id("popup")
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(300.0))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(GOLD_DIM))
            .shadow_lg()
            .text_color(rgb(TEXT))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(GOLD_DIM))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(GOLD))
                            .child(FLOORS[p.floor].title.to_uppercase()),
                    )
                    .child(
                        div()
                            .id("close-point")
                            .text_color(rgb(MUTED))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .child("✕")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.selected_point = None;
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    ),
            );
        if let Some(r) = p.room {
            let room = &d.rooms[r];
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .bg(rgb(CARD))
                    .text_xs()
                    .child(div().size(px(10.0)).bg(rgb(key_color(&room.key))).border_1().border_color(rgb(0x888888)))
                    .child(format!(
                        "{} — нужна {}{}",
                        room.name,
                        d.item_short(&room.key),
                        if room.approx { " (граница примерная)" } else { "" }
                    )),
            );
        }
        let mut list = div().flex().flex_col().px_3().py_2().gap_1();
        for it in items {
            let hit = self.is_shown(it);
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(if hit { TEXT } else { MUTED }))
                    .child(div().w(px(18.0)).child(GROUPS[it.group].icon))
                    .child(div().flex_1().child(it.short.clone()))
                    .child(
                        div()
                            .text_color(rgb(if hit { tier_color(it.price()) } else { MUTED }))
                            .child(fmt_short(it.price())),
                    )
                    .child({
                        let id = it.id.clone();
                        let hidden = self.hidden_items.contains(&id);
                        div()
                            .id(ElementId::Name(format!("hide-{id}").into()))
                            .ml_1()
                            .px_1()
                            .text_xs()
                            .border_1()
                            .border_color(rgb(GOLD_DIM))
                            .text_color(rgb(MUTED))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(TEXT)).border_color(rgb(GOLD)))
                            .child(if hidden { "вернуть" } else { "скрыть" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.hidden_items.remove(&id) {
                                    this.hidden_items.insert(id.clone());
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }))
                    }),
            );
        }
        card.child(list)
    }

    // ---------- меню ----------


    fn render_search(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.search_input.focus_handle(cx).is_focused(window);
        let empty = self.search.is_empty();
        div()
            .mt_4()
            .flex()
            .h(px(44.0))
            .border_1()
            .border_color(rgb(if focused { GOLD } else { GOLD_DIM }))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .px_3()
                    .overflow_hidden()
                    .text_color(rgb(TEXT))
                    .child(self.search_input.clone()),
            )
            .child(
                div()
                    .id("search-clear")
                    .w(px(44.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_l_1()
                    .border_color(rgb(GOLD_DIM))
                    .text_color(rgb(if empty { GOLD_DIM } else { TEXT }))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(CARD)))
                    .child(if empty { "⌕" } else { "✕" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.search_input.update(cx, |input, cx| input.clear(cx));
                    })),
            )
    }

    /// Строка категории в две колонки, как на mapgenie: иконка, название, счётчик.
    fn category_cell(
        id: ElementId,
        icon: &str,
        name: &str,
        count: usize,
        on: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let color = if on { TEXT } else { MUTED };
        div()
            .id(id)
            .w(px((SIDEBAR_W - PAD * 2.0) / 2.0 - 4.0))
            .flex()
            .items_center()
            .gap_2()
            .py_1()
            .pr_3()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(CARD)))
            .on_click(on_click)
            .child(div().w(px(22.0)).text_center().opacity(if on { 1.0 } else { 0.45 }).child(icon.to_string()))
            .child(
                div()
                    .flex_1()
                    .text_color(rgb(color))
                    .when(!on, |el| el.line_through())
                    .child(name.to_uppercase()),
            )
            .child(div().text_color(rgb(color)).child(count.to_string()))
    }

    fn toggle_section(key: &'static str, cx: &mut Context<Self>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _, _, cx| {
            if !this.collapsed.remove(key) {
                this.collapsed.insert(key);
            }
            cx.notify();
        })
    }

    fn render_sidebar(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open_price = !self.collapsed.contains("price");
        let open_access = !self.collapsed.contains("access");
        let open_loot = !self.collapsed.contains("loot");
        let open_loc = !self.collapsed.contains("loc");
        let mut col = div()
            .id("sidebar")
            .w(px(SIDEBAR_W))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .px(px(PAD))
            .py_4()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(GOLD_DIM))
            .text_color(rgb(TEXT))
            .overflow_y_scroll();

        // шапка: карта, режим, обновление
        col = col.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .pb_3()
                .border_b_1()
                .border_color(rgb(GOLD_DIM))
                .child(div().flex_1().text_xl().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child("ЛАБОРАТОРИЯ"))
                .child(segmented("mode-pve", "PvE", self.mode == Mode::Pve, cx.listener(|this, _, _, cx| this.set_mode(Mode::Pve, cx))))
                .child(segmented("mode-pvp", "PvP", self.mode == Mode::Pvp, cx.listener(|this, _, _, cx| this.set_mode(Mode::Pvp, cx))))
                .child(segmented("auto", "авто", self.auto_refresh, cx.listener(|this, _, _, cx| {
                    this.auto_refresh = !this.auto_refresh;
                    cx.notify();
                })))
                .child(
                    div()
                        .id("refresh")
                        .size(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(rgb(GOLD_DIM))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(CARD)))
                        .tooltip(tip("Обновить цены (скачать свежий дамп)"))
                        .child("⟳")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !matches!(this.status, Status::Busy(_)) {
                                this.load(true, cx);
                            }
                        })),
                ),
        );
        let status = match &self.status {
            Status::Busy(s) => div().text_color(rgb(GOLD)).child(s.clone()),
            Status::Error(e) => div().text_color(rgb(0xff6b6b)).child(format!("Ошибка: {e}")),
            Status::Idle => {
                let age = self.data.as_ref().and_then(|d| d.fetched_at).map(|t| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(t);
                    let mins = now.saturating_sub(t) / 60;
                    if mins < 60 {
                        format!("{mins} мин назад")
                    } else if mins < 60 * 48 {
                        format!("{} ч назад", mins / 60)
                    } else {
                        format!("{} дн назад", mins / 1440)
                    }
                });
                div().text_color(rgb(MUTED)).child(format!(
                    "Цены {} обновлены {} · барахолка (lastLow) или торговец",
                    self.mode.label(),
                    age.unwrap_or_else(|| "—".into())
                ))
            }
        };
        col = col.child(status.text_xs().mt_2());
        match &self.update {
            UpdateState::None => {}
            UpdateState::Available(u) => {
                col = col.child(
                    div()
                        .id("update")
                        .mt_3()
                        .px_3()
                        .py_2()
                        .flex()
                        .items_center()
                        .justify_between()
                        .bg(rgb(GOLD))
                        .text_color(rgb(0x1a1a1a))
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.9))
                        .on_click(cx.listener(|this, _, _, cx| this.install_update(cx)))
                        .child(div().font_weight(FontWeight::BOLD).child(format!("Доступна версия {}", u.version)))
                        .child("Обновить →"),
                );
            }
            UpdateState::Installing(v) => {
                col = col.child(div().mt_3().text_sm().text_color(rgb(GOLD)).child(format!("Ставлю {v}… приложение перезапустится")));
            }
            UpdateState::Failed(e) => {
                col = col.child(div().mt_3().text_xs().text_color(rgb(0xff6b6b)).child(format!("Обновление не удалось: {e}")));
            }
        }
        col = col.child(self.render_search(window, cx));

        // порог цены
        col = col.child(section_with_links("sec-price", "Дороже", open_price, Self::toggle_section("price", cx), div(), div())).when(open_price, |c| c.child(
            div().flex().flex_wrap().gap_1().children(THRESHOLDS.iter().map(|&t| {
                segmented(
                    ElementId::named_usize("thr", t as usize),
                    if t == 0 { "весь лут".to_string() } else { fmt_short(t) },
                    self.threshold == t,
                    cx.listener(move |this, _, _, cx| {
                        this.threshold = t;
                        cx.notify();
                    }),
                )
            })),
        ));

        let Some(d) = self.data.clone() else {
            return col;
        };

        // ---- ДОСТУП: где лежит лут (без ключа / за какой дверью) ----
        let mut access: HashMap<Option<String>, (usize, i64)> = HashMap::new();
        for (pi, p) in d.loot.iter().enumerate() {
            let shown: Vec<&Item> = p.items.iter().filter_map(|id| d.items.get(id)).filter(|i| self.is_shown(i)).collect();
            if shown.is_empty() {
                continue;
            }
            let e = access.entry(self.access_of(&d, pi)).or_default();
            e.0 += 1;
            e.1 += shown.iter().map(|i| i.price()).sum::<i64>();
        }
        // все ключи комнат, даже если сейчас там ничего нет
        for room in &d.rooms {
            access.entry(Some(room.key.clone())).or_default();
        }
        access.entry(None).or_default();
        let mut access: Vec<(Option<String>, (usize, i64))> = access.into_iter().collect();
        access.sort_by(|a, b| {
            a.0.is_some()
                .cmp(&b.0.is_some())
                .then(b.1 .1.cmp(&a.1 .1))
                .then(a.0.cmp(&b.0))
        });
        let all_access: Vec<Option<String>> = access.iter().map(|(k, _)| k.clone()).collect();
        col = col.child(section_with_links(
            "sec-access",
            "Доступ",
            open_access,
            Self::toggle_section("access", cx),
            link("acc-all", "все", cx.listener(|this, _, _, cx| {
                this.hidden_access.clear();
                cx.notify();
            })),
            link("acc-none", "ничего", cx.listener(move |this, _, _, cx| {
                this.hidden_access.extend(all_access.iter().cloned());
                cx.notify();
            })),
        ));
        for (i, (key, (n, sum))) in access.into_iter().enumerate().filter(|_| open_access) {
            let on = !self.hidden_access.contains(&key);
            let (name, color, key_price) = match &key {
                None => ("Без ключа".to_string(), None, None),
                Some(k) => (d.item_short(k), Some(key_color(k)), d.items.get(k).map(|i| i.price())),
            };
            let k2 = key.clone();
            let k3 = key.clone();
            col = col.child(
                div()
                    .id(ElementId::named_usize("acc", i))
                    .group("acc-row")
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(CARD)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.hidden_access.remove(&k2) {
                            this.hidden_access.insert(k2.clone());
                        }
                        cx.notify();
                    }))
                    .child(checkbox(if on { Check::On } else { Check::Off }))
                    .child(match color {
                        Some(c) => div().size(px(12.0)).bg(rgb(c)).border_1().border_color(rgb(0x999999)),
                        None => div().size(px(12.0)).child(""),
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(if on { TEXT } else { MUTED }))
                            .when(!on, |el| el.line_through())
                            .child(name),
                    )
                    .child(
                        div()
                            .id(ElementId::named_usize("acc-only", i))
                            .invisible()
                            .group_hover("acc-row", |s| s.visible())
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .hover(|s| s.text_color(rgb(GOLD)))
                            .child("только")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                // оставить только этот доступ
                                this.hidden_access = all_access_clone(&this.data, &k3);
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(match key_price {
                                Some(kp) => format!("{n} · {} · ключ {}", fmt_short(sum), fmt_short(kp)),
                                None => format!("{n} · {}", fmt_short(sum)),
                            }),
                    ),
            );
        }

        // ---- ЛУТ: категории, раскрываются до предметов ----
        // точки на предмет (с учётом фильтра доступа)
        let mut per_item: HashMap<&str, usize> = HashMap::new();
        for (pi, p) in d.loot.iter().enumerate() {
            if self.hidden_access.contains(&self.access_of(&d, pi)) {
                continue;
            }
            let mut seen = HashSet::new();
            for id in &p.items {
                if seen.insert(id.as_str()) {
                    *per_item.entry(id.as_str()).or_default() += 1;
                }
            }
        }
        let mut by_group: Vec<Vec<(&Item, usize)>> = vec![Vec::new(); GROUPS.len()];
        for (id, n) in per_item {
            if let Some(it) = d.items.get(id) {
                if self.matches_base(it) {
                    by_group[it.group].push((it, n));
                }
            }
        }
        for list in &mut by_group {
            list.sort_by(|a, b| b.0.price().cmp(&a.0.price()).then(a.0.short.cmp(&b.0.short)).then(a.0.id.cmp(&b.0.id)));
        }
        let total: usize = by_group.iter().map(|g| g.len()).sum();
        let all_groups: Vec<usize> = (0..GROUPS.len()).collect();
        col = col.child(section_with_links(
            "sec-loot",
            &format!("Лут · {total}"),
            open_loot,
            Self::toggle_section("loot", cx),
            link("g-all", "все", cx.listener(|this, _, _, cx| {
                this.hidden_groups.clear();
                this.hidden_items.clear();
                cx.notify();
            })),
            link("g-none", "ничего", cx.listener(move |this, _, _, cx| {
                this.hidden_groups.extend(all_groups.iter().copied());
                cx.notify();
            })),
        ));
        if total == 0 && open_loot {
            col = col.child(div().text_sm().text_color(rgb(MUTED)).child("Ничего не найдено"));
        }
        for (gi, list) in by_group.into_iter().enumerate().filter(|_| open_loot) {
            if list.is_empty() {
                continue;
            }
            let g = &GROUPS[gi];
            let group_on = !self.hidden_groups.contains(&gi);
            let n_on = list.iter().filter(|(i, _)| !self.hidden_items.contains(&i.id)).count();
            let state = if !group_on || n_on == 0 {
                Check::Off
            } else if n_on == list.len() {
                Check::On
            } else {
                Check::Partial
            };
            let points: usize = list.iter().filter(|(i, _)| group_on && !self.hidden_items.contains(&i.id)).map(|(_, n)| n).sum();
            let open = self.expanded.contains(&gi);
            let ids: Vec<String> = list.iter().map(|(i, _)| i.id.clone()).collect();
            let on = state != Check::Off;
            col = col.child(
                div()
                    .id(ElementId::named_usize("grp", gi))
                    .flex()
                    .items_center()
                    .gap_2()
                    .py(px(5.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(CARD)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded.remove(&gi) {
                            this.expanded.insert(gi);
                        }
                        cx.notify();
                    }))
                    .child(div().w(px(12.0)).text_color(rgb(GOLD_DIM)).child(if open { "▾" } else { "▸" }))
                    .child(
                        div()
                            .id(ElementId::named_usize("grp-check", gi))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if state == Check::Off {
                                    this.hidden_groups.remove(&gi);
                                    for id in &ids {
                                        this.hidden_items.remove(id);
                                    }
                                } else {
                                    this.hidden_groups.insert(gi);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }))
                            .child(checkbox(state)),
                    )
                    .child(div().w(px(22.0)).text_center().opacity(if on { 1.0 } else { 0.45 }).child(g.icon))
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(if on { TEXT } else { MUTED }))
                            .when(!on, |el| el.line_through())
                            .child(g.name.to_uppercase()),
                    )
                    .child(div().text_color(rgb(if on { TEXT } else { MUTED })).child(format!("{} / {}", list.len(), points))),
            );
            if !open {
                continue;
            }
            let group_ids: Vec<String> = list.iter().map(|(i, _)| i.id.clone()).collect();
            for (item, n) in list {
                let item_on = group_on && !self.hidden_items.contains(&item.id);
                let id = item.id.clone();
                let others = group_ids.clone();
                col = col.child(
                    div()
                        .id(ElementId::Name(format!("item-{}", item.id).into()))
                        .flex()
                        .items_center()
                        .gap_2()
                        .pl(px(34.0))
                        .py_1()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(CARD)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.hidden_groups.contains(&gi) {
                                // группа была выключена: включаем только этот предмет
                                this.hidden_groups.remove(&gi);
                                this.hidden_items.extend(others.iter().filter(|o| **o != id).cloned());
                                this.hidden_items.remove(&id);
                            } else if !this.hidden_items.remove(&id) {
                                this.hidden_items.insert(id.clone());
                            }
                            cx.notify();
                        }))
                        .child(checkbox(if item_on { Check::On } else { Check::Off }))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_color(rgb(if item_on { TEXT } else { MUTED }))
                                        .when(!item_on, |el| el.line_through())
                                        .child(item.short.clone()),
                                )
                                .child(div().text_xs().text_color(rgb(MUTED)).child(format!("{} · {} · {} точ.", item.name, item.price_source(), n))),
                        )
                        .child(div().text_color(rgb(if item_on { tier_color(item.price()) } else { MUTED })).child(fmt_short(item.price()))),
                );
            }
        }

        // ---- ЛОКАЦИИ ----
        col = col.child(section_with_links(
            "sec-loc",
            "Локации",
            open_loc,
            Self::toggle_section("loc", cx),
            link("rooms-reload", "перечитать зоны", cx.listener(|this, _, _, cx| this.load(false, cx))),
            div(),
        ));
        col = col.when(open_loc, |col| col.child(
            div()
                .flex()
                .flex_wrap()
                .gap_x_2()
                .child(Self::category_cell("l-doors".into(), "🔒", "Двери", d.locks.len(), self.show_doors, cx.listener(|this, _, _, cx| {
                    this.show_doors = !this.show_doors;
                    cx.notify();
                })))
                .child(Self::category_cell("l-rooms".into(), "🚪", "Комнаты", d.rooms.len(), self.show_rooms, cx.listener(|this, _, _, cx| {
                    this.show_rooms = !this.show_rooms;
                    cx.notify();
                })))
                .child(Self::category_cell("l-exits".into(), "🏃", "Выходы", d.extracts.len(), self.show_extracts, cx.listener(|this, _, _, cx| {
                    this.show_extracts = !this.show_extracts;
                    cx.notify();
                }))),
        ));
        col
    }
}

/// Все варианты доступа, кроме `keep` — для кнопки «только».
fn all_access_clone(data: &Option<Rc<LabData>>, keep: &Option<String>) -> HashSet<Option<String>> {
    let mut all: HashSet<Option<String>> = HashSet::new();
    all.insert(None);
    if let Some(d) = data {
        for r in &d.rooms {
            all.insert(Some(r.key.clone()));
        }
    }
    all.remove(keep);
    all
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    On,
    Off,
    Partial,
}

fn checkbox(state: Check) -> impl IntoElement {
    div()
        .size(px(16.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(rgb(if state == Check::Off { GOLD_DIM } else { GOLD }))
        .bg(rgb(if state == Check::On { GOLD } else { PANEL }))
        .text_xs()
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(if state == Check::On { 0x1a1a1a } else { GOLD }))
        .child(match state {
            Check::On => "✓",
            Check::Partial => "–",
            Check::Off => "",
        })
}

fn section_with_links(
    id: &'static str,
    title: &str,
    open: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    a: impl IntoElement,
    b: impl IntoElement,
) -> impl IntoElement {
    div()
        .mt_5()
        .mb_2()
        .flex()
        .items_end()
        .justify_between()
        .child(
            div()
                .id(id)
                .flex()
                .gap_2()
                .text_sm()
                .text_color(rgb(GOLD))
                .cursor_pointer()
                .hover(|s| s.text_color(rgb(TEXT)))
                .on_click(on_toggle)
                .child(div().w(px(10.0)).child(if open { "▾" } else { "▸" }))
                .child(title.to_uppercase()),
        )
        .when(open, |el| el.child(div().flex().gap_3().text_xs().child(a).child(b)))
}


fn map_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(34.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(GOLD_DIM))
        .text_color(rgb(TEXT))
        .text_lg()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(CARD)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |ev, window, cx| {
            on_click(ev, window, cx);
            cx.stop_propagation();
        })
        .child(label)
}

fn segmented(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .px_2()
        .py_1()
        .text_sm()
        .border_1()
        .border_color(rgb(if active { GOLD } else { GOLD_DIM }))
        .bg(rgb(if active { GOLD } else { PANEL }))
        .text_color(rgb(if active { 0x1a1a1a } else { TEXT }))
        .cursor_pointer()
        .hover(|s| s.border_color(rgb(GOLD)))
        .on_click(on_click)
        .child(label.into())
}

fn link(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .text_color(rgb(MUTED))
        .cursor_pointer()
        .hover(|s| s.text_color(rgb(GOLD)))
        .on_click(on_click)
        .child(label)
}

impl Render for HelperApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.persist();
        div()
            .flex()
            .size_full()
            .bg(rgb(BG))
            .font_family(if cfg!(target_os = "windows") { "Segoe UI" } else { ".SystemUIFont" })
            .child(self.render_sidebar(window, cx))
            .child(self.render_map(cx))
    }
}
