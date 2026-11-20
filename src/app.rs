//! Главное окно: меню в стиле mapgenie слева и карта Лабы (три этажа рядом).

use gpui::{
    canvas, div, img, point, prelude::*, svg, Hsla, PathBuilder, px, rgb, rgba, AnyElement, App, Bounds, ClickEvent, Context,
    ElementId, Entity, FocusHandle, Focusable, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
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
pub(crate) const BG: u32 = 0x111111;
pub(crate) const PANEL: u32 = 0x1c1b19;
pub(crate) const CARD: u32 = 0x262420;
pub(crate) const GOLD: u32 = 0xc2ab74;
pub(crate) const GOLD_DIM: u32 = 0x6f6248;
pub(crate) const TEXT: u32 = 0xebe3cf;
pub(crate) const MUTED: u32 = 0x7d7666;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Maps,
    Scanner,
    Raids,
    Settings,
}

/// Сочетания сканера.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyRole {
    Scan,
    Hide,
    Raid,
}

impl KeyRole {
    const ALL: [KeyRole; 3] = [KeyRole::Scan, KeyRole::Hide, KeyRole::Raid];

    fn default_key(self) -> &'static str {
        match self {
            KeyRole::Scan => "f9",
            KeyRole::Hide => "f10",
            KeyRole::Raid => "f8",
        }
    }

    fn reset_id(self) -> &'static str {
        match self {
            KeyRole::Scan => "hk-scan-reset",
            KeyRole::Hide => "hk-hide-reset",
            KeyRole::Raid => "hk-raid-reset",
        }
    }

    fn key(self, sc: &mut settings::ScannerSettings) -> &mut String {
        match self {
            KeyRole::Scan => &mut sc.scan_key,
            KeyRole::Hide => &mut sc.hide_key,
            KeyRole::Raid => &mut sc.raid_key,
        }
    }
}

enum UpdateState {
    None,
    /// Идёт ручная проверка.
    Checking,
    /// Ручная проверка: новой версии нет.
    UpToDate,
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
    /// Проверять обновления приложения при запуске.
    auto_update_check: bool,
    /// Открытый раздел приложения.
    section: Section,
    /// Открыт список выбора карты.
    map_menu_open: bool,
    scanner: settings::ScannerSettings,
    /// Какое сочетание сканера сейчас записывается.
    recording: Option<KeyRole>,
    /// Подсказка, почему сочетание не записано (например, голая ПКМ).
    hotkey_error: Option<&'static str>,
    hotkey_focus: FocusHandle,
    /// Журнал рейдов (общий с оверлеем после рейда).
    raid_log: Entity<crate::raid_overlay::RaidLog>,
    /// Сканер цен: хуки, карточка поверх игры, итог последнего скана.
    scanner_rt: Entity<crate::scanner::Scanner>,
    /// Фильтр статистики рейдов по режиму: None — все.
    raids_mode: Option<&'static str>,
    /// Раскрытый рейд в истории.
    open_raid: Option<usize>,
    /// Пример предмета в превью оверлея.
    preview_item: usize,
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

pub(crate) fn tier_color(price: i64) -> u32 {
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

pub(crate) fn with_alpha(color: u32, alpha: u32) -> gpui::Rgba {
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
        let raid_log = cx.new(|_| crate::raid_overlay::RaidLog::load());
        cx.observe(&raid_log, |_, _, cx| cx.notify()).detach();
        let scanner_rt = cx.new(|cx| crate::scanner::Scanner::new(st.scanner.clone(), raid_log.clone(), cx));
        cx.observe(&scanner_rt, |_, _, cx| cx.notify()).detach();
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
            auto_update_check: st.auto_update_check,
            section: match st.section.as_str() {
                "scanner" => Section::Scanner,
                "raids" => Section::Raids,
                "settings" => Section::Settings,
                _ => Section::Maps,
            },
            map_menu_open: false,
            recording: None,
            hotkey_error: None,
            hotkey_focus: cx.focus_handle(),
            preview_item: 0,
            raid_log,
            scanner_rt,
            raids_mode: None,
            open_raid: None,
            scanner: st.scanner.clone(),
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
        if this.auto_update_check {
            this.check_updates(false, cx);
        }
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

    /// Проверка обновлений: `manual` — по кнопке (показываем и «актуально», и ошибки).
    fn check_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if matches!(self.update, UpdateState::Checking | UpdateState::Installing(_)) {
            return;
        }
        if manual {
            self.update = UpdateState::Checking;
            cx.notify();
        }
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { if manual { crate::updater::check_now() } else { crate::updater::check() } })
                .await;
            this.update(cx, |this, cx| {
                this.update = match found {
                    Ok(Some(update)) => UpdateState::Available(std::sync::Arc::new(update)),
                    Ok(None) if manual => UpdateState::UpToDate,
                    Err(e) if manual => UpdateState::Failed(format!("{e:#}")),
                    _ => UpdateState::None,
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn install_update(&mut self, cx: &mut Context<Self>) {
        if !crate::updater::can_install() {
            self.update = UpdateState::Failed("в dev-сборке обновление не ставится — скачай релиз".into());
            cx.notify();
            return;
        }
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
            auto_update_check: self.auto_update_check,
            section: match self.section {
                Section::Maps => "maps",
                Section::Scanner => "scanner",
                Section::Raids => "raids",
                Section::Settings => "settings",
            }
            .into(),
            map: "the-lab".into(),
            scanner: self.scanner.clone(),
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

    /// Переключает этажи, сохраняя зум и место на карте под центром окна.
    fn set_floor_view(&mut self, view: Option<usize>, cx: &mut Context<Self>) {
        self.hover = None;
        let bounds = *self.map_bounds.borrow();
        let (Some(b), Some(range), true) = (bounds, self.range, self.fitted) else {
            // карта ещё не показана — просто вписываем
            self.floor_view = view;
            self.fit();
            cx.notify();
            return;
        };
        let (floor_w, _) = range.pixel_size();
        let center = (f32::from(b.size.width) / 2.0, f32::from(b.size.height) / 2.0);
        // точка карты под центром: этаж и координата внутри картинки этажа
        let world_x = (center.0 - self.offset.0) / self.scale;
        let world_y = (center.1 - self.offset.1) / self.scale;
        let (from_floor, local_x) = match self.floor_view {
            Some(f) => (f, world_x),
            None => {
                let step = floor_w + FLOOR_GAP;
                let f = ((world_x / step).floor().max(0.0) as usize).min(FLOORS.len() - 1);
                (f, world_x - f as f32 * step)
            }
        };
        self.floor_view = view;
        // на «Все этажи» остаёмся на том же этаже, на одиночном — то же место на выбранном
        let to_floor = view.unwrap_or(from_floor);
        let new_world_x = self.floor_origin(to_floor) + local_x;
        self.offset = (center.0 - new_world_x * self.scale, center.1 - world_y * self.scale);
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
                .child(
                    div()
                        .id("map-picker")
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .text_xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(GOLD))
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.map_menu_open = !this.map_menu_open;
                            cx.notify();
                        }))
                        .child("ЛАБОРАТОРИЯ")
                        .child(div().text_sm().child(if self.map_menu_open { "▴" } else { "▾" })),
                )
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
        if self.map_menu_open {
            col = col.child(self.render_map_menu(cx));
        }
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
            UpdateState::None | UpdateState::Checking | UpdateState::UpToDate => {}
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
pub(crate) enum Check {
    On,
    Off,
    Partial,
}

pub(crate) fn checkbox(state: Check) -> impl IntoElement {
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

pub(crate) fn segmented(
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

pub(crate) fn link(
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
        let (sc, data, recording) = (self.scanner.clone(), self.data.clone(), self.recording.is_some());
        let mode = if self.mode == Mode::Pvp { "pvp" } else { "pve" };
        self.scanner_rt.update(cx, |s, _| s.sync(&sc, data, mode, recording));
        div()
            .flex()
            .size_full()
            .bg(rgb(BG))
            .font_family(if cfg!(target_os = "windows") { "Segoe UI" } else { ".SystemUIFont" })
            .child(self.render_rail(cx))
            .map(|root| match self.section {
                Section::Maps => root.child(self.render_sidebar(window, cx)).child(self.render_map(cx)),
                Section::Scanner => root.child(self.render_scanner(cx)),
                Section::Raids => root.child(self.render_raids(cx)),
                Section::Settings => root.child(self.render_settings(cx)),
            })
    }
}

// ---------- разделы: навигация, выбор карты, сканер цен ----------


impl HelperApp {
    /// Узкая колонка слева с разделами.
    fn render_rail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |id: &'static str, icon: &'static str, label: &'static str, section: Section, active: bool, cx: &mut Context<Self>| {
            div()
                .id(id)
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .py_3()
                .cursor_pointer()
                .border_l_2()
                .border_color(rgb(if active { GOLD } else { 0x0e0e0e }))
                .bg(rgb(if active { CARD } else { 0x0e0e0e }))
                .hover(|s| s.bg(rgb(CARD)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.section = section;
                    this.hover = None;
                    cx.notify();
                }))
                .child(div().text_xl().child(icon))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(if active { GOLD } else { MUTED }))
                        .child(label),
                )
        };
        div()
            .w(px(72.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .pt_2()
            .bg(rgb(0x0e0e0e))
            .border_r_1()
            .border_color(rgb(GOLD_DIM))
            .child(item("nav-maps", "🗺", "КАРТЫ", Section::Maps, self.section == Section::Maps, cx))
            .child(item("nav-scanner", "🔍", "СКАНЕР", Section::Scanner, self.section == Section::Scanner, cx))
            .child(item("nav-raids", "📊", "РЕЙДЫ", Section::Raids, self.section == Section::Raids, cx))
            .child(div().flex_1())
            .child(item("nav-settings", "⚙", "НАСТРОЙКИ", Section::Settings, self.section == Section::Settings, cx))
    }

    /// Выпадающий список карт под заголовком.
    fn render_map_menu(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .mt_2()
            .flex()
            .flex_col()
            .border_1()
            .border_color(rgb(GOLD_DIM))
            .bg(rgb(CARD))
            .children(crate::maps::MAPS.iter().enumerate().map(|(i, m)| {
                let current = m.id == "the-lab";
                div()
                    .id(ElementId::named_usize("map-opt", i))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py(px(6.0))
                    .text_color(rgb(if m.available { TEXT } else { MUTED }))
                    .when(current, |el| el.bg(rgb(0x302c24)))
                    .when(m.available, |el| {
                        el.cursor_pointer().hover(|s| s.bg(rgb(0x302c24))).on_click(cx.listener(|this, _, _, cx| {
                            this.map_menu_open = false;
                            cx.notify();
                        }))
                    })
                    .child(m.name)
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(if current { GOLD } else { MUTED }))
                            .child(if current { "✓" } else if m.available { "" } else { "скоро" }),
                    )
            }))
    }

    /// Страница настроек сканера цен.
    fn render_scanner(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let sc = self.scanner.clone();
        let supported = cfg!(target_os = "windows");

        let toggle_row = |id: &'static str, label: &'static str, hint: &'static str, on: bool, f: fn(&mut settings::ScannerSettings), cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_3()
                .py(px(6.0))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(CARD)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    f(&mut this.scanner);
                    cx.notify();
                }))
                .child(checkbox(if on { Check::On } else { Check::Off }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().text_color(rgb(if on { TEXT } else { MUTED })).child(label))
                        .when(!hint.is_empty(), |el| el.child(div().text_xs().text_color(rgb(MUTED)).child(hint))),
                )
        };
        let heading = |t: &'static str| div().mt_6().mb_2().text_sm().text_color(rgb(GOLD)).child(t);

        let rt = self.scanner_rt.read(cx);
        let keys = |k: &str| crate::hotkey::display(k).join(" + ");
        let (title, body, warn): (String, String, bool) = if !supported {
            (
                "Сканер работает только на Windows".into(),
                "Игра запускается только на Windows, поэтому сканер будет там. Здесь можно заранее настроить, что он будет показывать.".into(),
                true,
            )
        } else if !sc.enabled {
            ("Сканер выключен".into(), "Включи его ниже — клавиши начнут работать сразу.".into(), true)
        } else if rt.languages.as_ref().is_some_and(|l| l.is_empty()) {
            ("Нет языка распознавания текста".into(), crate::scanner::NO_OCR.into(), true)
        } else {
            let langs = rt.languages.as_ref().map(|l| l.join(", ")).unwrap_or_else(|| "…".into());
            (
                "Сканер работает".into(),
                format!(
                    "{} — цена предмета под курсором, {} — рюкзак после рейда{}. Распознавание: {langs}.",
                    keys(&sc.scan_key),
                    keys(&sc.raid_key),
                    if sc.only_in_game { ", только в окне игры" } else { "" }
                ),
                false,
            )
        };
        let last = rt.status.clone();
        let status_card = div()
            .mt_4()
            .p_3()
            .border_1()
            .border_color(rgb(if warn { 0x7a5a2a } else { GOLD_DIM }))
            .bg(rgb(CARD))
            .flex()
            .flex_col()
            .gap_1()
            .child(div().font_weight(FontWeight::BOLD).text_color(rgb(if warn { 0xffb74d } else { GOLD })).child(title))
            .child(div().text_sm().text_color(rgb(MUTED)).child(body))
            .when_some(last.filter(|_| supported), |el, (ok, text)| {
                el.child(div().mt_1().text_sm().text_color(rgb(if ok { TEXT } else { 0xffb74d })).child(text))
            });

        let seconds: [(u32, &'static str); 4] = [(3, "3 с"), (5, "5 с"), (10, "10 с"), (0, "пока не скрою")];
        let positions: [(&'static str, &'static str); 2] = [("cursor", "у курсора"), ("corner", "в углу экрана")];

        div()
            .id("scanner-page")
            .when(self.recording.is_some(), |el| {
                // во время записи любой клик — это кандидат в сочетание, а не нажатие кнопок страницы
                el.capture_any_mouse_down(cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    let Some(role) = this.recording else { return };
                    cx.stop_propagation();
                    match crate::hotkey::from_mouse(ev.button, &ev.modifiers) {
                        Ok(new) => {
                            this.assign_hotkey(role, new);
                            window.blur();
                        }
                        // обычный ЛКМ — отмена записи, как клик мимо поля
                        Err(_) if ev.button == MouseButton::Left => {
                            this.recording = None;
                            window.blur();
                        }
                        Err(msg) => this.hotkey_error = Some(msg),
                    }
                    cx.notify();
                }))
            })
            .flex_1()
            .flex()
            .items_start()
            .gap(px(32.0))
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(PANEL))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .w(px(640.0))
                    .flex_none()
                    .px(px(32.0))
                    .py(px(24.0))
                    .child(div().text_xl().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child("СКАНЕР ЦЕН"))
                    .child(
                        div()
                            .mt_1()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("Наведи курсор на предмет в игре и нажми клавишу — рядом появится его цена. Вторая клавиша считает весь рюкзак под курсором и открывает «После рейда»."),
                    )
                    .child(status_card)
                    .child(heading("ОСНОВНОЕ"))
                    .child(toggle_row("sc-enabled", "Включить сканер", "Сканер слушает горячую клавишу, пока открыт Tarkov Helper", sc.enabled, |s| s.enabled = !s.enabled, cx))
                    .child(toggle_row("sc-game", "Только в окне игры", "Клавиши срабатывают, когда впереди Escape from Tarkov", sc.only_in_game, |s| s.only_in_game = !s.only_in_game, cx))
                    .child(heading("ГОРЯЧИЕ КЛАВИШИ"))
                    .child(self.hotkey_field("hk-scan", "Цена предмета под курсором", &sc.scan_key, KeyRole::Scan, cx))
                    .child(self.hotkey_field("hk-hide", "Скрыть оверлей", &sc.hide_key, KeyRole::Hide, cx))
                    .child(self.hotkey_field("hk-raid", "Рюкзак после рейда", &sc.raid_key, KeyRole::Raid, cx))
                    .child(
                        div()
                            .mt_1()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("Нажми на поле, потом клавишу или кнопку мыши: Ctrl + Shift + F9, Alt + Q, Mouse 4, колесо. ЛКМ/ПКМ — только с Ctrl/Alt/Shift. Esc — отмена."),
                    )
                    .when_some(self.hotkey_error, |el, msg| {
                        el.child(div().mt_1().text_xs().text_color(rgb(0xffb74d)).child(msg))
                    })
                    .child(
                        div(),
                    )
                    .child(heading("ЧТО ПОКАЗЫВАТЬ"))
                    .child(toggle_row("sc-slot", "Цена за клетку", "Чтобы понять, стоит ли предмет места в рюкзаке", sc.show_per_slot, |s| s.show_per_slot = !s.show_per_slot, cx))
                    .child(toggle_row("sc-flea", "Цена на барахолке", "lastLow — самый дешёвый лот", sc.show_flea, |s| s.show_flea = !s.show_flea, cx))
                    .child(toggle_row("sc-trader", "Лучший торговец", "Кому выгоднее сдать и за сколько", sc.show_trader, |s| s.show_trader = !s.show_trader, cx))
                    .child(toggle_row("sc-needed", "Нужен для убежища и задач", "", sc.show_needed, |s| s.show_needed = !s.show_needed, cx))
                    .child(heading("ОВЕРЛЕЙ"))
                    .child(toggle_row(
                        "sc-hold",
                        "Показывать, пока держу клавишу",
                        "Отпустил клавишу сканирования — оверлей пропал",
                        sc.hold_to_show,
                        |s| s.hold_to_show = !s.hold_to_show,
                        cx,
                    ))
                    .map(|el| {
                        if sc.hold_to_show {
                            el.child(
                                div()
                                    .mt_1()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child("В режиме удержания таймер и скрытие при движении мыши не нужны."),
                            )
                        } else {
                            el.child(toggle_row("sc-move", "Скрывать, когда двигаю мышь", "", sc.hide_on_mouse_move, |s| s.hide_on_mouse_move = !s.hide_on_mouse_move, cx))
                                .child(div().text_sm().mt_2().mb_1().child("Сколько показывать"))
                                .child(div().flex().flex_wrap().gap_1().children(seconds.into_iter().enumerate().map(|(i, (v, label))| {
                                    segmented(
                                        ElementId::named_usize("sc-sec", i),
                                        label,
                                        sc.show_seconds == v,
                                        cx.listener(move |this, _, _, cx| {
                                            this.scanner.show_seconds = v;
                                            cx.notify();
                                        }),
                                    )
                                })))
                        }
                    })
                    .child(
                        div()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("Оверлей после рейда висит всегда — пока не подтвердишь или не отменишь."),
                    )
                    .child(div().text_sm().mt_3().mb_1().child("Где показывать"))
                    .child(div().flex().flex_wrap().gap_1().children(positions.into_iter().enumerate().map(|(i, (v, label))| {
                        segmented(
                            ElementId::named_usize("sc-pos", i),
                            label,
                            sc.position == v,
                            cx.listener(move |this, _, _, cx| {
                                this.scanner.position = v.to_string();
                                cx.notify();
                            }),
                        )
                    })))
                    .child(heading("ОТЛАДКА"))
                    .child(toggle_row(
                        "sc-debug",
                        "Сохранять снимки сканов",
                        "Папка scanner-debug в папке данных: снимок вокруг курсора, найденные квадраты и прочитанный текст",
                        sc.debug_shots,
                        |s| s.debug_shots = !s.debug_shots,
                        cx,
                    ))
                    .child(heading("КАК ЭТО РАБОТАЕТ"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("1. По клавише делается снимок окна игры.")
                            .child("2. На снимке находятся квадраты предметов, и в правом верхнем углу каждого распознаётся короткое имя.")
                            .child("3. Название сверяется со списком предметов, цена берётся из тех же данных, что и карта.")
                            .child("Только снимок экрана — в память игры приложение не лезет. Игру нужно запускать в режиме «оконный без рамки»."),
                    ),
            )
            .child(self.render_overlay_preview(cx))
    }
}

impl HelperApp {
    /// Поле горячей клавиши: клик — запись следующего сочетания.
    fn hotkey_field(
        &self,
        id: &'static str,
        label: &'static str,
        combo: &str,
        role: KeyRole,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let recording = self.recording == Some(role);
        let default = role.default_key();
        let is_default = combo.eq_ignore_ascii_case(default);
        let keycap = |k: String| {
            div()
                .px_2()
                .py(px(2.0))
                .border_1()
                .border_b_2()
                .border_color(rgb(GOLD_DIM))
                .bg(rgb(0x2c2924))
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(TEXT))
                .child(k)
        };
        let caps = crate::hotkey::display(combo);
        let n = caps.len();

        let field = div()
            .id(id)
            .w(px(300.0))
            .h(px(40.0))
            .px_3()
            .flex()
            .items_center()
            .gap_1()
            .border_1()
            .border_color(rgb(if recording { GOLD } else { GOLD_DIM }))
            .bg(rgb(if recording { 0x2c2924 } else { PANEL }))
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(GOLD)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.recording = Some(role);
                this.hotkey_error = None;
                this.hotkey_focus.focus(window);
                cx.notify();
            }))
            .when(recording, |el| {
                el.track_focus(&self.hotkey_focus)
                    .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                        let k = &ev.keystroke;
                        let plain = !(k.modifiers.control || k.modifiers.alt || k.modifiers.shift || k.modifiers.platform);
                        if k.key == "escape" && plain {
                            this.recording = None;
                            window.blur();
                        } else if let Some(new) = crate::hotkey::from_keystroke(k) {
                            this.assign_hotkey(role, new);
                            window.blur();
                        } else {
                            return; // только модификатор — ждём основную клавишу
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
            })
            .map(|el| {
                if recording {
                    el.child(div().text_sm().text_color(rgb(GOLD)).child("Нажми сочетание…"))
                        .child(div().flex_1())
                        .child(div().text_xs().text_color(rgb(MUTED)).child("Esc — отмена"))
                } else {
                    el.children(caps.into_iter().enumerate().flat_map(move |(i, k)| {
                        let mut v: Vec<AnyElement> = vec![keycap(k).into_any_element()];
                        if i + 1 < n {
                            v.push(div().text_xs().text_color(rgb(MUTED)).child("+").into_any_element());
                        }
                        v
                    }))
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(rgb(MUTED)).child("изменить"))
                }
            });

        div()
            .mt_2()
            .flex()
            .items_center()
            .gap_3()
            .child(div().w(px(240.0)).text_sm().child(label))
            .child(field)
            .when(!is_default && !recording, |el| {
                el.child(link(
                    role.reset_id(),
                    "по умолчанию",
                    cx.listener(move |this, _, _, cx| {
                        this.assign_hotkey(role, default.to_string());
                        cx.notify();
                    }),
                ))
            })
    }
}

/// Предметы для примера в превью оверлея.
const PREVIEW_ITEMS: [(&str, &str); 4] = [
    ("5c0530ee86f774697952d952", "LEDX"),
    ("57347ca924597744596b4e71", "GPU"),
    ("5c1d0f4986f7744bb01837fa", "Черная"),
    ("59faff1d86f7746c51718c9c", "0.2BTC"),
];

impl HelperApp {
    /// Превью: как оверлей сканера будет выглядеть поверх игры.
    fn render_overlay_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let col = div().flex_1().min_w(px(420.0)).max_w(px(640.0)).pr(px(32.0)).py(px(24.0));
        let Some(d) = self.data.clone() else {
            return col.child(div().text_color(rgb(MUTED)).child("Загружаю данные для превью…"));
        };
        let (item_id, _) = PREVIEW_ITEMS[self.preview_item.min(PREVIEW_ITEMS.len() - 1)];
        let Some(item) = d.items.get(item_id).cloned() else {
            return col.child(div().text_color(rgb(MUTED)).child("Предмет для превью не найден в данных"));
        };
        let sc = &self.scanner;

        let needs = d.needs.get(item_id).cloned().unwrap_or_default();
        let footer = crate::scanner::card::footer_text(sc);
        let card = crate::scanner::card::price_card(&item, &needs, 0, sc, &footer).0.shadow_lg();

        // макет экрана: сетка инвентаря, предмет под курсором, оверлей
        const CELL: f32 = 48.0;
        let (screen_w, screen_h) = (560.0, 540.0);
        let (tile_x, tile_y) = (CELL * 2.0 + 16.0, CELL + 16.0);
        let (tw, th) = (CELL * item.width as f32, CELL * item.height as f32);
        let mut screen = div()
            .relative()
            .w(px(screen_w))
            .h(px(screen_h))
            .overflow_hidden()
            .bg(rgb(0x1a1b1c))
            .border_1()
            .border_color(rgb(GOLD_DIM));
        for gy in 0..11 {
            for gx in 0..11 {
                screen = screen.child(
                    div()
                        .absolute()
                        .left(px(16.0 + gx as f32 * CELL))
                        .top(px(16.0 + gy as f32 * CELL))
                        .size(px(CELL - 2.0))
                        .bg(rgb(0x232425))
                        .border_1()
                        .border_color(rgb(0x2e3031)),
                );
            }
        }
        screen = screen
            .child(
                div()
                    .absolute()
                    .left(px(tile_x))
                    .top(px(tile_y))
                    .w(px(tw - 2.0))
                    .h(px(th - 2.0))
                    .bg(rgb(0x3a3d40))
                    .border_1()
                    .border_color(rgb(0x9aa0a6))
                    .flex()
                    .items_end()
                    .justify_end()
                    .p(px(2.0))
                    .child(div().text_size(px(9.0)).text_color(rgb(0xdddddd)).child(item.short.clone())),
            )
            .child(
                div()
                    .absolute()
                    .left(px(tile_x + tw * 0.6))
                    .top(px(tile_y + th * 0.6))
                    .text_lg()
                    .text_color(rgb(0xffffff))
                    .child("➤"),
            );
        screen = if sc.position == "corner" {
            screen.child(div().absolute().top(px(12.0)).right(px(12.0)).child(card))
        } else {
            screen.child(div().absolute().left(px(tile_x + tw + 14.0)).top(px(tile_y + 10.0)).child(card))
        };

        col.child(div().text_sm().text_color(rgb(GOLD)).child("КАК ЭТО ВЫГЛЯДИТ"))
            .child(
                div()
                    .mt_1()
                    .mb_3()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Пример с реальными ценами и требованиями — меняется вместе с настройками слева."),
            )
            .child(div().mb_3().flex().gap_1().children(PREVIEW_ITEMS.iter().enumerate().map(|(i, (_, label))| {
                segmented(
                    ElementId::named_usize("pv-item", i),
                    *label,
                    self.preview_item == i,
                    cx.listener(move |this, _, _, cx| {
                        this.preview_item = i;
                        cx.notify();
                    }),
                )
            })))
            .child(screen)
    }
}

impl HelperApp {
    /// Раздел «Рейды»: средний профит по картам и история.
    fn render_raids(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let raids = self.raid_log.read(cx).raids.clone();
        let mode = self.raids_mode;
        let stats = crate::raids::stats(&raids, mode);
        let map_name = |id: &str| {
            crate::maps::MAPS.iter().find(|m| m.id == id).map(|m| m.name).unwrap_or("?").to_string()
        };
        let heading = |t: &'static str| div().mt_6().mb_2().text_sm().text_color(rgb(GOLD)).child(t);

        // таблица по картам, по убыванию среднего профита
        let mut rows: Vec<(String, crate::raids::MapStats)> = stats.into_iter().collect();
        rows.sort_by(|a, b| b.1.avg().cmp(&a.1.avg()));
        let cell = |t: String, w: f32, color: u32, bold: bool| {
            div()
                .w(px(w))
                .text_color(rgb(color))
                .when(bold, |el| el.font_weight(FontWeight::BOLD))
                .child(t)
        };
        let header = div()
            .flex()
            .py_1()
            .border_b_1()
            .border_color(rgb(GOLD_DIM))
            .text_xs()
            .child(cell("КАРТА".into(), 200.0, MUTED, false))
            .child(cell("РЕЙДОВ".into(), 90.0, MUTED, false))
            .child(cell("СРЕДНИЙ ПРОФИТ".into(), 160.0, MUTED, false))
            .child(cell("ЛУЧШИЙ".into(), 130.0, MUTED, false))
            .child(cell("ВСЕГО".into(), 130.0, MUTED, false));
        let table = div().flex().flex_col().child(header).children(rows.iter().map(|(map, st)| {
            div()
                .flex()
                .py(px(6.0))
                .border_b_1()
                .border_color(rgb(0x2a2824))
                .child(cell(map_name(map), 200.0, TEXT, true))
                .child(cell(st.raids.to_string(), 90.0, TEXT, false))
                .child(cell(fmt_rub(st.avg()), 160.0, tier_color(st.avg()), true))
                .child(cell(fmt_short(st.best), 130.0, TEXT, false))
                .child(cell(fmt_short(st.total), 130.0, MUTED, false))
        }));

        // история, новые сверху
        let history: Vec<AnyElement> = raids.iter().enumerate().rev().filter(|(_, r)| mode.is_none_or(|m| r.mode == m)).map(|(i, r)| {
            let open = self.open_raid == Some(i);
            let when = chrono::DateTime::from_timestamp(r.ts as i64, 0)
                .map(|d| d.with_timezone(&chrono::Local).format("%d.%m %H:%M").to_string())
                .unwrap_or_default();
            div()
                .flex()
                .flex_col()
                .border_b_1()
                .border_color(rgb(0x2a2824))
                .child(
                    div()
                        .id(ElementId::named_usize("raid", i))
                        .flex()
                        .items_center()
                        .gap_3()
                        .py(px(6.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(CARD)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_raid = if this.open_raid == Some(i) { None } else { Some(i) };
                            cx.notify();
                        }))
                        .child(div().w(px(12.0)).text_color(rgb(GOLD_DIM)).child(if open { "▾" } else { "▸" }))
                        .child(div().w(px(100.0)).text_sm().text_color(rgb(MUTED)).child(when))
                        .child(div().w(px(150.0)).font_weight(FontWeight::SEMIBOLD).child(map_name(&r.map)))
                        .child(div().w(px(50.0)).text_xs().text_color(rgb(MUTED)).child(r.mode.to_uppercase()))
                        .child(div().w(px(90.0)).text_sm().text_color(rgb(MUTED)).child(format!("{} предм.", r.items.len())))
                        .child(div().flex_1().font_weight(FontWeight::BOLD).text_color(rgb(tier_color(r.total()))).child(fmt_rub(r.total())))
                        .child(
                            div()
                                .id(ElementId::named_usize("raid-del", i))
                                .px_2()
                                .text_color(rgb(MUTED))
                                .hover(|s| s.text_color(rgb(0xff6b6b)))
                                .child("✕")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.raid_log.update(cx, |log, cx| log.remove(i, cx));
                                    this.open_raid = None;
                                    cx.stop_propagation();
                                })),
                        ),
                )
                .when(open, |el| {
                    el.child(div().pl(px(28.0)).pb_2().flex().flex_col().gap(px(2.0)).children(r.items.iter().map(|it| {
                        div()
                            .flex()
                            .text_sm()
                            .child(div().w(px(220.0)).child(if it.count > 1 { format!("{} ×{}", it.short, it.count) } else { it.short.clone() }))
                            .child(div().text_color(rgb(MUTED)).child(fmt_rub(it.price * it.count as i64)))
                    })))
                })
                .into_any_element()
        }).collect();

        let modes: [(&'static str, &'static str, Option<&'static str>); 3] =
            [("rm-all", "Все", None), ("rm-pve", "PvE", Some("pve")), ("rm-pvp", "PvP", Some("pvp"))];
        let mode_chips: Vec<AnyElement> = modes
            .into_iter()
            .map(|(id, label, value)| {
                segmented(id, label, self.raids_mode == value, cx.listener(move |this, _, _, cx| {
                    this.raids_mode = value;
                    cx.notify();
                }))
                .into_any_element()
            })
            .collect();

        div()
            .id("raids-page")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(PANEL))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .max_w(px(820.0))
                    .px(px(32.0))
                    .py(px(24.0))
                    .child(div().text_xl().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child("РЕЙДЫ"))
                    .child(
                        div()
                            .mt_1()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("После рейда наведись на рюкзак и нажми хоткей сканера: оверлей распознает предметы, ты уберёшь лишнее, выберешь карту — и рейд попадёт в статистику."),
                    )
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .id("raid-sample")
                                    .px_4()
                                    .py_2()
                                    .bg(rgb(GOLD))
                                    .text_color(rgb(0x1a1a1a))
                                    .font_weight(FontWeight::BOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .child("Открыть оверлей после рейда (пример)")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let Some(d) = this.data.clone() else { return };
                                        let rows = crate::raid_overlay::sample_backpack(&d);
                                        let mode = if this.mode == Mode::Pvp { "pvp" } else { "pve" }.to_string();
                                        crate::raid_overlay::open(rows, mode, this.raid_log.clone(), true, cx);
                                    })),
                            )
                            .child(div().text_xs().text_color(rgb(MUTED)).child("Настоящее распознавание — в сканере под Windows")),
                    )
                    .child(
                        div()
                            .mt_6()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().text_color(rgb(GOLD)).child("СРЕДНИЙ ПРОФИТ ПО КАРТАМ"))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .children(mode_chips),
                            ),
                    )
                    .child(if rows.is_empty() {
                        div().mt_2().text_sm().text_color(rgb(MUTED)).child("Пока нет рейдов — подтверди первый в оверлее.")
                    } else {
                        div().mt_2().child(table)
                    })
                    .child(heading("ИСТОРИЯ"))
                    .children(history),
            )
    }
}

impl HelperApp {
    /// Общие настройки приложения.
    fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let heading = |t: &'static str| div().mt_6().mb_2().text_sm().text_color(rgb(GOLD)).child(t);
        let row = |label: &'static str, hint: String| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .py(px(8.0))
                .border_b_1()
                .border_color(rgb(0x2a2824))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().child(label))
                        .when(!hint.is_empty(), |el| el.child(div().text_xs().text_color(rgb(MUTED)).child(hint))),
                )
        };
        let button = |id: &'static str, label: String, primary: bool| {
            div()
                .id(id)
                .flex_none()
                .px_3()
                .py_1()
                .border_1()
                .border_color(rgb(if primary { GOLD } else { GOLD_DIM }))
                .bg(rgb(if primary { GOLD } else { PANEL }))
                .text_color(rgb(if primary { 0x1a1a1a } else { TEXT }))
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .hover(|s| s.opacity(0.9))
                .child(label)
        };
        let toggle = |id: &'static str, on: bool| {
            div()
                .id(id)
                .flex_none()
                .w(px(40.0))
                .h(px(22.0))
                .p(px(2.0))
                .rounded_full()
                .cursor_pointer()
                .bg(rgb(if on { GOLD } else { 0x3a372f }))
                .flex()
                .when(on, |el| el.justify_end())
                .child(div().size(px(18.0)).rounded_full().bg(rgb(if on { 0x1a1a1a } else { MUTED })))
        };

        // статус обновления
        let (status_text, status_color): (String, u32) = match &self.update {
            UpdateState::None => ("Нажми, чтобы проверить".into(), MUTED),
            UpdateState::Checking => ("Проверяю…".into(), GOLD),
            UpdateState::UpToDate => ("У тебя последняя версия".into(), 0x7cb342),
            UpdateState::Available(u) => (format!("Доступна версия {}", u.version), GOLD),
            UpdateState::Installing(v) => (format!("Ставлю {v}… приложение перезапустится"), GOLD),
            UpdateState::Failed(e) => (format!("Не получилось: {e}"), 0xff6b6b),
        };
        let update_button = match &self.update {
            UpdateState::Available(_) => button("st-install", "Обновить".into(), true)
                .on_click(cx.listener(|this, _, _, cx| this.install_update(cx)))
                .into_any_element(),
            UpdateState::Checking | UpdateState::Installing(_) => div().into_any_element(),
            _ => button("st-check", "Проверить обновления".into(), false)
                .on_click(cx.listener(|this, _, _, cx| this.check_updates(true, cx)))
                .into_any_element(),
        };

        let fetched = self.data.as_ref().and_then(|d| d.fetched_at).map(|t| {
            chrono::DateTime::from_timestamp(t as i64, 0)
                .map(|d| d.with_timezone(&chrono::Local).format("%d.%m %H:%M").to_string())
                .unwrap_or_default()
        });
        let data_dir = data::data_dir();
        let build = if cfg!(debug_assertions) { "dev-сборка" } else { "релиз" };

        div()
            .id("settings-page")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(PANEL))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .max_w(px(720.0))
                    .px(px(32.0))
                    .py(px(24.0))
                    .child(div().text_xl().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child("НАСТРОЙКИ"))
                    // --- приложение ---
                    .child(heading("ПРИЛОЖЕНИЕ"))
                    .child(
                        row("Tarkov Helper", format!("версия {} · {build}", env!("CARGO_PKG_VERSION")))
                            .child(div().flex().items_center().gap_3().child(div().text_sm().text_color(rgb(status_color)).child(status_text)).child(update_button)),
                    )
                    .child(
                        row("Проверять обновления при запуске", "Если выйдет новая версия, в меню появится кнопка «Обновить»".into())
                            .child(toggle("st-autoupd", self.auto_update_check).on_click(cx.listener(|this, _, _, cx| {
                                this.auto_update_check = !this.auto_update_check;
                                cx.notify();
                            }))),
                    )
                    // --- данные ---
                    .child(heading("ДАННЫЕ И ЦЕНЫ"))
                    .child(
                        row("Режим игры", "От режима зависят цены и данные карт".into()).child(
                            div()
                                .flex()
                                .gap_1()
                                .child(segmented("st-pve", "PvE", self.mode == Mode::Pve, cx.listener(|this, _, _, cx| this.set_mode(Mode::Pve, cx))))
                                .child(segmented("st-pvp", "PvP", self.mode == Mode::Pvp, cx.listener(|this, _, _, cx| this.set_mode(Mode::Pvp, cx)))),
                        ),
                    )
                    .child(
                        row("Автообновление цен", "При запуске, если данным больше часа, и каждые 30 минут".into())
                            .child(toggle("st-autoref", self.auto_refresh).on_click(cx.listener(|this, _, _, cx| {
                                this.auto_refresh = !this.auto_refresh;
                                cx.notify();
                            }))),
                    )
                    .child(
                        row(
                            "Цены",
                            match (&self.status, fetched) {
                                (Status::Busy(s), _) => s.clone(),
                                (_, Some(t)) => format!("обновлены {t} · источник tarkov.dev"),
                                _ => "ещё не загружены".into(),
                            },
                        )
                        .child(button("st-refresh", "Обновить сейчас".into(), false).on_click(cx.listener(|this, _, _, cx| {
                            if !matches!(this.status, Status::Busy(_)) {
                                this.load(true, cx);
                            }
                        }))),
                    )
                    .child(
                        row("Папка с данными", data_dir.display().to_string()).child(
                            button("st-folder", if cfg!(target_os = "macos") { "Показать в Finder" } else { "Открыть папку" }.into(), false)
                                .on_click(move |_, _, cx| cx.reveal_path(&data_dir)),
                        ),
                    )
                    // --- ссылки ---
                    .child(heading("ССЫЛКИ"))
                    .child(
                        row("Исходный код и релизы", format!("github.com/{}", crate::updater::REPO)).child(
                            button("st-github", "Открыть GitHub".into(), false)
                                .on_click(|_, _, cx| cx.open_url(&format!("https://github.com/{}", crate::updater::REPO))),
                        ),
                    )
                    .child(
                        row("Данные о предметах и картах", "tarkov.dev — открытые данные сообщества".into())
                            .child(button("st-tdev", "tarkov.dev".into(), false).on_click(|_, _, cx| cx.open_url("https://tarkov.dev"))),
                    ),
            )
    }
}

impl HelperApp {
    /// Назначить сочетание сканированию или скрытию; занятое другим действием — поменять местами.
    /// Записывает сочетание; если оно уже занято другим действием — меняет их местами.
    fn assign_hotkey(&mut self, role: KeyRole, new: String) {
        let sc = &mut self.scanner;
        let old = role.key(sc).clone();
        for other in KeyRole::ALL.into_iter().filter(|r| *r != role) {
            if other.key(sc).eq_ignore_ascii_case(&new) {
                *other.key(sc) = old.clone();
            }
        }
        *role.key(sc) = new;
        self.recording = None;
        self.hotkey_error = None;
    }
}
