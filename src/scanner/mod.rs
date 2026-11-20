//! Сканер цен: по горячей клавише снимок экрана → квадраты предметов → OCR ярлыков → цена.
//! Один предмет под курсором — карточка рядом с ним; весь контейнер — окно «После рейда».
//! Только снимок экрана: в память игры не лезем.

pub mod card;
pub mod matcher;
pub mod vision;
#[cfg(windows)]
pub mod win;

use std::rc::Rc;

use gpui::{AppContext as _, Context, Entity};
use image::{Rgba, RgbaImage};

use crate::data::LabData;
use crate::raid_overlay::RaidLog;
use crate::settings::ScannerSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// Нажата клавиша сканирования.
    Scan,
    /// Отпущена клавиша сканирования (режим «пока держу»).
    ScanUp,
    Hide,
    /// Скан всего контейнера под курсором → «После рейда».
    Raid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Item,
    Raid,
}

/// Квадрат предмета и прочитанный в его ярлыке текст (по движку OCR на язык).
#[derive(Clone, Debug)]
pub struct Seen {
    pub b: vision::ItemBox,
    pub texts: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ScanOutput {
    pub seen: Vec<Seen>,
    /// Левый верхний угол снимка на экране.
    pub origin: (i32, i32),
    /// Курсор на экране.
    pub cursor: (i32, i32),
    /// Найденный размер клетки, если сетка нашлась.
    pub cell: Option<f32>,
    pub ms: u128,
}

pub const NO_OCR: &str = "В Windows нет языка распознавания текста: Параметры → Время и язык → Язык и регион → добавь English (United States) или Русский";

/// Картинка ярлыка для OCR: правый верхний угол квадрата, увеличенный втрое, с полями.
/// Ярлык — белый текст на тёмном фоне предмета: переводим в тёмный текст на белом.
/// Варианты подобраны на эталонном скрине (тест `tune_label_preprocessing`): основной читает все
/// 11 ярлыков, запасной — другой контраст для одиночного скана, если основной не узнали.
pub fn label_image(img: &RgbaImage, b: &vision::ItemBox, cell: f32) -> RgbaImage {
    label_image_with(img, b, cell, 60.0, 140.0, 2)
}

#[allow(dead_code)] // только Windows
fn label_image_alt(img: &RgbaImage, b: &vision::ItemBox, cell: f32) -> RgbaImage {
    label_image_with(img, b, cell, 80.0, 150.0, 3)
}

fn label_image_with(img: &RgbaImage, b: &vision::ItemBox, cell: f32, lo: f32, hi: f32, scale: u32) -> RgbaImage {
    let (x, y, w, h) = b.label_rect(cell);
    let w = w.min(img.width().saturating_sub(x)).max(1);
    let h = h.min(img.height().saturating_sub(y)).max(1);
    const PADDING: u32 = 8;
    let mut out = RgbaImage::from_pixel(w + PADDING * 2, h + PADDING * 2, Rgba([255, 255, 255, 255]));
    for sy in 0..h {
        for sx in 0..w {
            let p = img.get_pixel(x + sx, y + sy).0;
            let m = p[0].min(p[1]).min(p[2]) as f32;
            let v = ((m - lo) / (hi - lo)).clamp(0.0, 1.0);
            let c = (255.0 * (1.0 - v)) as u8;
            out.put_pixel(sx + PADDING, sy + PADDING, Rgba([c, c, c, 255]));
        }
    }
    image::imageops::resize(&out, out.width() * scale, out.height() * scale, image::imageops::FilterType::CatmullRom)
}

/// Снимок экрана и распознавание. Вызывать в фоновом потоке.
#[cfg(windows)]
pub fn scan(kind: Kind, debug: Option<std::path::PathBuf>) -> anyhow::Result<ScanOutput> {
    let t0 = std::time::Instant::now();
    let shot = win::capture_at_cursor()?;
    let gray = image::imageops::grayscale(&shot.img);
    let (w, h) = shot.img.dimensions();
    let hint = vision::cell_hint(w, h);
    let (cx, cy) = shot.cursor;
    let found = match kind {
        Kind::Item => vision::item_at(&gray, cx, cy, hint).map(|(b, l)| (vec![b], l)),
        Kind::Raid => vision::items_around(&gray, cx, cy, hint),
    };
    let engines = win::ocr_engines();
    if engines.is_empty() {
        anyhow::bail!(NO_OCR);
    }
    let mut seen = Vec::new();
    if let Some((boxes, lat)) = &found {
        for b in boxes {
            let read = |label: &RgbaImage| -> Vec<String> {
                engines.iter().map(|(_, e)| win::recognize(e, label).unwrap_or_default().trim().to_string()).collect()
            };
            let mut texts = read(&label_image(&shot.img, b, lat.cell));
            // один предмет — можно позволить второй вариант картинки
            if kind == Kind::Item {
                texts.extend(read(&label_image_alt(&shot.img, b, lat.cell)));
            }
            seen.push(Seen { b: *b, texts });
        }
    }
    let out = ScanOutput {
        seen,
        origin: (shot.left, shot.top),
        cursor: (shot.left + cx as i32, shot.top + cy as i32),
        cell: found.as_ref().map(|f| f.1.cell),
        ms: t0.elapsed().as_millis(),
    };
    if let Some(dir) = debug {
        if let Err(e) = dump_debug(&dir, &shot.img, shot.cursor, &out) {
            eprintln!("scanner debug: {e:#}");
        }
    }
    Ok(out)
}

/// Снимок для отладки: область вокруг курсора с найденными квадратами и текстом OCR.
#[allow(dead_code)]
fn dump_debug(dir: &std::path::Path, img: &RgbaImage, cursor: (u32, u32), out: &ScanOutput) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let cell = out.cell.unwrap_or(84.0) as i64;
    let (mut x0, mut y0) = (cursor.0 as i64 - cell * 4, cursor.1 as i64 - cell * 4);
    let (mut x1, mut y1) = (cursor.0 as i64 + cell * 4, cursor.1 as i64 + cell * 4);
    for s in &out.seen {
        x0 = x0.min(s.b.x0 as i64 - cell);
        y0 = y0.min(s.b.y0 as i64 - cell);
        x1 = x1.max(s.b.x1 as i64 + cell);
        y1 = y1.max(s.b.y1 as i64 + cell);
    }
    let x0 = x0.clamp(0, img.width() as i64 - 1) as u32;
    let y0 = y0.clamp(0, img.height() as i64 - 1) as u32;
    let x1 = x1.clamp(x0 as i64 + 1, img.width() as i64) as u32;
    let y1 = y1.clamp(y0 as i64 + 1, img.height() as i64) as u32;
    let mut crop = image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
    let mut rect = |bx0: u32, by0: u32, bx1: u32, by1: u32, color: Rgba<u8>| {
        for t in 0..2 {
            for x in bx0..bx1 {
                for y in [by0 + t, by1.saturating_sub(1 + t)] {
                    if x >= x0 && y >= y0 && x < x1 && y < y1 {
                        crop.put_pixel(x - x0, y - y0, color);
                    }
                }
            }
            for y in by0..by1 {
                for x in [bx0 + t, bx1.saturating_sub(1 + t)] {
                    if x >= x0 && y >= y0 && x < x1 && y < y1 {
                        crop.put_pixel(x - x0, y - y0, color);
                    }
                }
            }
        }
    };
    for s in &out.seen {
        rect(s.b.x0, s.b.y0, s.b.x1, s.b.y1, Rgba([80, 220, 120, 255]));
    }
    rect(cursor.0.saturating_sub(3), cursor.1.saturating_sub(3), cursor.0 + 3, cursor.1 + 3, Rgba([255, 60, 60, 255]));
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    crop.save(dir.join(format!("scan-{stamp}.png")))?;
    let mut txt = format!(
        "снимок {}×{}, курсор {:?}, клетка {:?}, {} мс\n",
        img.width(),
        img.height(),
        cursor,
        out.cell,
        out.ms
    );
    for s in &out.seen {
        txt.push_str(&format!("{}×{} ({},{})-({},{}) exact={} → {:?}\n", s.b.w, s.b.h, s.b.x0, s.b.y0, s.b.x1, s.b.y1, s.b.exact, s.texts));
    }
    std::fs::write(dir.join(format!("scan-{stamp}.txt")), txt)?;
    // храним последние 40 снимков
    let mut files: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    files.sort();
    let excess = files.len().saturating_sub(80);
    for f in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(f);
    }
    Ok(())
}

/// Состояние сканера в приложении: хуки, карточка поверх игры, последний результат.
pub struct Scanner {
    pub settings: ScannerSettings,
    data: Option<Rc<LabData>>,
    mode: String,
    log: Entity<RaidLog>,
    /// Пока записывается сочетание, хоткеи сканера молчат.
    recording: bool,
    /// Что показать на странице сканера: итог последнего скана или ошибка.
    pub status: Option<(bool, String)>,
    /// Языки OCR в системе (None — ещё не проверили).
    pub languages: Option<Vec<String>>,
    #[allow(dead_code)]
    busy: bool,
    #[allow(dead_code)]
    held: bool,
    /// Номер показа карточки: старые таймеры по нему понимают, что устарели.
    #[allow(dead_code)]
    generation: u64,
    #[allow(dead_code)]
    visible: bool,
    #[cfg(windows)]
    card: Option<(gpui::WindowHandle<card::ScanCard>, isize)>,
}

impl Scanner {
    pub fn new(settings: ScannerSettings, log: Entity<RaidLog>, cx: &mut Context<Self>) -> Self {
        let this = Self {
            settings,
            data: None,
            mode: "pve".into(),
            log,
            recording: false,
            status: None,
            languages: None,
            busy: false,
            held: false,
            generation: 0,
            visible: false,
            #[cfg(windows)]
            card: None,
        };
        #[cfg(windows)]
        {
            use futures::StreamExt;
            if let Some(mut rx) = win::start_hooks() {
                cx.spawn(async move |this, cx| {
                    while let Some(ev) = rx.next().await {
                        if this.update(cx, |this, cx| this.on_event(ev, cx)).is_err() {
                            break;
                        }
                    }
                })
                .detach();
            }
            this.configure_hooks();
            // языки OCR — в фоне, движки создаются не мгновенно
            cx.spawn(async move |this, cx| {
                let langs = cx
                    .background_executor()
                    .spawn(async { win::ocr_engines().iter().map(|(t, _)| t.clone()).collect::<Vec<_>>() })
                    .await;
                this.update(cx, |this, cx| {
                    this.languages = Some(langs);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(not(windows))]
        let _ = cx;
        this
    }

    /// Свежие настройки, данные и режим из главного окна.
    pub fn sync(&mut self, settings: &ScannerSettings, data: Option<Rc<LabData>>, mode: &str, recording: bool) {
        self.data = data;
        if self.mode != mode {
            self.mode = mode.to_string();
        }
        if self.settings != *settings || self.recording != recording {
            self.settings = settings.clone();
            self.recording = recording;
            #[cfg(windows)]
            self.configure_hooks();
        }
    }

    #[cfg(windows)]
    fn configure_hooks(&self) {
        let sc = &self.settings;
        win::configure(win::HookConfig {
            enabled: sc.enabled && !self.recording,
            only_in_game: sc.only_in_game,
            scan: win::parse_combo(&sc.scan_key),
            hide: win::parse_combo(&sc.hide_key),
            raid: win::parse_combo(&sc.raid_key),
        });
    }

    #[cfg(windows)]
    fn set_status(&mut self, ok: bool, text: impl Into<String>, cx: &mut Context<Self>) {
        self.status = Some((ok, text.into()));
        cx.notify();
    }

    #[cfg(windows)]
    fn on_event(&mut self, ev: HotkeyEvent, cx: &mut Context<Self>) {
        match ev {
            HotkeyEvent::Scan => {
                self.held = true;
                self.start_scan(Kind::Item, cx);
            }
            HotkeyEvent::ScanUp => self.held = false,
            HotkeyEvent::Hide => self.hide_card(cx),
            HotkeyEvent::Raid => self.start_scan(Kind::Raid, cx),
        }
    }

    /// Прячет карточку. Вызовы Win32 для окон — вне обновления GPUI (в следующем такте),
    /// иначе оконная процедура GPUI может попасть внутрь занятого состояния приложения.
    #[cfg(windows)]
    fn hide_card(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        if let Some((_, hwnd)) = self.card {
            cx.spawn(async move |_, _| win::hide(hwnd)).detach();
        }
    }

    #[cfg(windows)]
    fn start_scan(&mut self, kind: Kind, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.data.is_none() {
            self.set_status(false, "Данные о предметах ещё загружаются", cx);
            return;
        }
        self.busy = true;
        self.generation += 1;
        let generation = self.generation;
        let debug = self.settings.debug_shots.then(|| crate::data::data_dir().join("scanner-debug"));
        cx.spawn(async move |this, cx| {
            let out = cx.background_executor().spawn(async move { scan(kind, debug) }).await;
            let place = this
                .update(cx, |this, cx| {
                    this.busy = false;
                    match kind {
                        Kind::Item => this.show_item(out, cx),
                        Kind::Raid => {
                            this.open_raid(out, cx);
                            None
                        }
                    }
                })
                .ok()
                .flatten();
            let Some(place) = place else { return };
            if place.fresh {
                win::prepare_card(place.hwnd);
            }
            win::show_at(place.hwnd, place.x, place.y, place.w, place.h);

            // когда спрятать: таймер, движение мыши или отпущенная клавиша
            let shown = std::time::Instant::now();
            let anchor = win::cursor_pos();
            let held_at_show = this.read_with(cx, |s, _| s.held).unwrap_or(false);
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
                let Ok((current, visible, sc, held)) =
                    this.read_with(cx, |s, _| (s.generation, s.visible, s.settings.clone(), s.held))
                else {
                    return;
                };
                if current != generation || !visible {
                    return;
                }
                let t = shown.elapsed();
                let hide = if sc.hold_to_show {
                    // отпустил раньше, чем появилась карточка, — покажем её коротко
                    if held_at_show { !held } else { t.as_millis() > 1500 }
                } else {
                    let timer = sc.show_seconds > 0 && t.as_secs() >= sc.show_seconds as u64;
                    let moved = sc.hide_on_mouse_move && t.as_millis() > 250 && {
                        let p = win::cursor_pos();
                        (p.0 - anchor.0).pow(2) + (p.1 - anchor.1).pow(2) > 40 * 40
                    };
                    timer || moved
                };
                if hide {
                    this.update(cx, |s, _| s.visible = false).ok();
                    win::hide(place.hwnd);
                    return;
                }
            }
        })
        .detach();
    }

    /// Сопоставляет прочитанное с предметами. Первый текст, который узнали, — по языкам OCR.
    fn identify<'a>(m: &matcher::Matcher<'a>, s: &Seen) -> Option<matcher::Found<'a>> {
        s.texts.iter().filter(|t| !t.is_empty()).find_map(|t| m.find(t, s.b.w, s.b.h, s.b.exact))
    }

    /// Готовит карточку предмета под курсором. Возвращает, куда её поставить.
    #[cfg(windows)]
    fn show_item(&mut self, out: anyhow::Result<ScanOutput>, cx: &mut Context<Self>) -> Option<CardPlace> {
        let out = match out {
            Ok(o) => o,
            Err(e) => {
                self.set_status(false, format!("{e:#}"), cx);
                return None;
            }
        };
        let data = self.data.clone()?;
        let m = matcher::Matcher::new(&data.items);
        let seen = out.seen.first();
        let content = match seen.and_then(|s| Self::identify(&m, s)) {
            Some(f) => {
                self.set_status(true, format!("{} — {} ({} мс)", f.item.short, crate::data::fmt_rub(f.item.price()), out.ms), cx);
                card::CardContent::Found {
                    needs: data.needs.get(&f.item.id).cloned().unwrap_or_default(),
                    item: f.item.clone(),
                    others: f.others,
                }
            }
            None => {
                let read = seen.and_then(|s| s.texts.iter().find(|t| !t.is_empty()).cloned()).unwrap_or_default();
                let why = match (seen, out.cell) {
                    (Some(_), _) => format!("не узнал ярлык «{read}»"),
                    (None, Some(_)) => "под курсором пустая клетка".into(),
                    (None, None) => "не нашёл сетку инвентаря под курсором".into(),
                };
                self.set_status(false, format!("Последний скан: {why} ({} мс)", out.ms), cx);
                card::CardContent::Missing { read }
            }
        };
        let footer = card::footer_text(&self.settings);

        let fresh = self.card.is_none();
        if fresh {
            let options = gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(
                    gpui::point(gpui::px(0.0), gpui::px(0.0)),
                    gpui::size(gpui::px(card::CARD_W), gpui::px(200.0)),
                ))),
                titlebar: None,
                kind: gpui::WindowKind::PopUp,
                focus: false,
                show: false,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            };
            let settings = self.settings.clone();
            let handle = cx
                .open_window(options, |_, cx| cx.new(|_| card::ScanCard { content, settings, footer: footer.clone() }))
                .ok()?;
            let hwnd = handle.update(cx, |_, window, _| win::window_hwnd(window)).ok().flatten()?;
            self.card = Some((handle, hwnd));
        } else if let Some((handle, _)) = &self.card {
            let settings = self.settings.clone();
            handle
                .update(cx, |card, _, cx| {
                    card.content = content;
                    card.settings = settings;
                    card.footer = footer;
                    cx.notify();
                })
                .ok()?;
        }
        let (handle, hwnd) = self.card.clone()?;
        let height = handle.update(cx, |card, _, _| card.height()).ok()?;

        let scale = win::scale_of(hwnd);
        let (w, h) = ((card::CARD_W * scale).ceil() as i32, (height * scale).ceil() as i32);
        let work = win::monitor_rect(out.cursor.0, out.cursor.1, true);
        let gap = (12.0 * scale) as i32;
        let (x, y) = if self.settings.position == "corner" {
            (work.right - w - gap, work.top + gap)
        } else {
            // справа от квадрата предмета, если не влезает — слева
            let (bx0, by0, bx1) = match seen {
                Some(s) => (out.origin.0 + s.b.x0 as i32, out.origin.1 + s.b.y0 as i32, out.origin.0 + s.b.x1 as i32),
                None => (out.cursor.0, out.cursor.1, out.cursor.0 + gap),
            };
            let x = if bx1 + gap + w <= work.right { bx1 + gap } else { bx0 - gap - w };
            (x.clamp(work.left, (work.right - w).max(work.left)), by0.clamp(work.top, (work.bottom - h).max(work.top)))
        };
        self.visible = true;
        Some(CardPlace { hwnd, x, y, w, h, fresh })
    }

    /// Весь контейнер → окно «После рейда». Одинаковые предметы — одной строкой с количеством.
    #[cfg(windows)]
    fn open_raid(&mut self, out: anyhow::Result<ScanOutput>, cx: &mut Context<Self>) {
        use crate::raid_overlay::Detected;
        let out = match out {
            Ok(o) => o,
            Err(e) => return self.set_status(false, format!("{e:#}"), cx),
        };
        let Some(data) = self.data.clone() else { return };
        let m = matcher::Matcher::new(&data.items);
        let mut rows: Vec<Detected> = Vec::new();
        for s in &out.seen {
            match Self::identify(&m, s) {
                Some(f) => match rows.iter_mut().find(|r| r.item.as_ref().is_some_and(|i| i.id == f.item.id)) {
                    Some(r) => r.count += 1,
                    None => rows.push(Detected { label: f.item.short.clone(), item: Some(f.item.clone()), count: 1, included: true }),
                },
                None => rows.push(Detected {
                    item: None,
                    label: s.texts.iter().find(|t| !t.is_empty()).cloned().unwrap_or_else(|| "?".into()),
                    count: 1,
                    included: false,
                }),
            }
        }
        if rows.is_empty() {
            let why = if out.cell.is_some() { "в контейнере под курсором нет предметов" } else { "не нашёл сетку инвентаря под курсором" };
            return self.set_status(false, format!("Скан рюкзака: {why}"), cx);
        }
        let known = rows.iter().filter(|r| r.item.is_some()).count();
        self.set_status(true, format!("Скан рюкзака: узнал {known} из {} ({} мс)", out.seen.len(), out.ms), cx);
        crate::raid_overlay::open(rows, self.mode.clone(), self.log.clone(), false, cx);
    }
}

/// Куда поставить карточку (физические пиксели экрана).
#[cfg(windows)]
struct CardPlace {
    hwnd: isize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    /// Окно только что создано — нужно донастроить стили.
    fresh: bool,
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn tune_label_preprocessing() {
        let engines = win::ocr_engines();
        let img = image::load_from_memory(include_bytes!("../../tools/scanner-prototype/backpack.png")).unwrap().to_rgba8();
        let gray = image::imageops::grayscale(&img);
        let (boxes, lat) = vision::items_around(&gray, 300, 450, 84.0).unwrap();
        for (lo, hi) in [(100.0, 210.0), (90.0, 170.0), (80.0, 150.0), (120.0, 180.0), (60.0, 140.0)] {
            for scale in [1, 2, 3, 4] {
                let mut got = Vec::new();
                for b in boxes.iter().filter(|b| b.x0 >= 200) {
                    let label = label_image_with(&img, b, lat.cell, lo, hi, scale);
                    let t: Vec<String> = engines.iter().map(|(_, e)| win::recognize(e, &label).unwrap_or_default()).collect();
                    got.push(t.join("|"));
                }
                eprintln!("{lo}-{hi} x{scale}: {got:?}");
            }
        }
    }

    /// OCR Windows на ярлыках эталонного рюкзака и сопоставление с кэшем дампа PvE.
    /// Без языков OCR или без кэша — пропускается (печатает, почему).
    #[test]
    fn backpack_labels_ocr() {
        let engines = win::ocr_engines();
        if engines.is_empty() {
            eprintln!("пропуск: {NO_OCR}");
            return;
        }
        let img = image::load_from_memory(include_bytes!("../../tools/scanner-prototype/backpack.png")).unwrap().to_rgba8();
        let gray = image::imageops::grayscale(&img);
        let (boxes, lat) = vision::items_around(&gray, 300, 450, 84.0).unwrap();
        let data = crate::data::load_lab(crate::data::Mode::Pve).ok();
        let m = data.as_ref().map(|d| matcher::Matcher::new(&d.items));
        let mut known = Vec::new();
        for b in boxes.iter().filter(|b| b.x0 >= 200) {
            let label = label_image(&img, b, lat.cell);
            if let Ok(dir) = std::env::var("SCAN_DUMP") {
                let _ = label.save(format!("{dir}/label-{}-{}.png", b.x0, b.y0));
            }
            let texts: Vec<String> =
                engines.iter().map(|(_, e)| win::recognize(e, &label).unwrap_or_default().trim().to_string()).collect();
            let seen = Seen { b: *b, texts: texts.clone() };
            let found = m.as_ref().and_then(|m| Scanner::identify(m, &seen)).map(|f| f.item.short_en.clone());
            eprintln!("{}×{} {:?} → {:?}", b.w, b.h, texts, found);
            known.extend(found);
        }
        if data.is_none() {
            eprintln!("пропуск сопоставления: нет кэша дампа PvE");
            return;
        }
        for name in ["Sausage", "Roler", "0.2BTC", "Wedge", "Wires", "SAS", "SVDS", "Salewa", "Augmentin"] {
            assert!(known.iter().any(|k| k == name), "не узнан {name}: {known:?}");
        }
    }
}
