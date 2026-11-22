//! Оверлей после рейда: распознанный рюкзак → исключить лишнее → выбрать карту → подтвердить.
//! Открывается хоткеем сканера (скан рюкзака под курсором, Windows) или с примером со страницы рейдов.
//! Висит, пока не подтвердишь, не отменишь или не кликнешь мимо (это тоже отмена, рейд не
//! записывается): настройки удержания, таймера и скрытия при движении мыши из `ScannerSettings`
//! на него не действуют.

use gpui::{
    div, prelude::*, px, rgb, size, App, Bounds, Context, ElementId, Entity, FontWeight, SharedString,
    Window, WindowBounds, WindowKind, WindowOptions,
};

use crate::app::{checkbox, segmented, tier_color, Check, CARD, GOLD, GOLD_DIM, MUTED, PANEL, TEXT};
use crate::data::{fmt_rub, fmt_short, Item, MapData};
use crate::maps::MAPS;
use crate::raids::{self, Raid, RaidItem};

/// Прозрачные поля вокруг панели оверлея — под нашу тень.
const OVERLAY_MARGIN: f32 = 16.0;

/// Журнал рейдов, общий для главного окна и оверлеев.
pub struct RaidLog {
    pub raids: Vec<Raid>,
}

impl RaidLog {
    pub fn load() -> Self {
        Self { raids: raids::load() }
    }

    pub fn add(&mut self, raid: Raid, cx: &mut Context<Self>) {
        self.raids.push(raid);
        raids::save(&self.raids);
        cx.notify();
    }

    pub fn remove(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.raids.len() {
            self.raids.remove(index);
            raids::save(&self.raids);
            cx.notify();
        }
    }
}

/// Строка распознанного рюкзака.
#[derive(Clone)]
pub struct Detected {
    /// None — квадрат найден, но предмет не опознан (жетон, мусор OCR).
    pub item: Option<Item>,
    /// Что прочитал OCR.
    pub label: String,
    pub count: u32,
    pub included: bool,
}

pub struct RaidOverlay {
    rows: Vec<Detected>,
    map: Option<&'static str>,
    mode: String,
    log: Entity<RaidLog>,
    /// Это пример, а не настоящий скан.
    sample: bool,
    /// Слежка за кликом мимо; кончается вместе с окном.
    #[cfg(windows)]
    outside: Option<crate::scanner::win::OutsideClicks>,
}

/// Пример: рюкзак со скриншота пользователя (распознан прототипом).
pub fn sample_backpack(data: &MapData) -> Vec<Detected> {
    let found = [
        ("635a758bfefc88a93f021b8a", 1),
        ("59faf7ca86f7740dbe19f6c2", 1),
        ("59faff1d86f7746c51718c9c", 1),
        ("6ad9cf0b5394c084310cac90", 1),
        ("5c06779c86f77426e00dd782", 3),
        ("590c37d286f77443be3d7827", 1),
        ("5c46fbd72e2216398b5a8c9c", 1),
        ("544fb45d4bdc2dee738b4568", 1),
        ("590c695186f7741e566b64a2", 1),
    ];
    let mut rows: Vec<Detected> = found
        .iter()
        .filter_map(|&(id, count)| {
            let it = data.items.get(id)?.clone();
            Some(Detected { label: it.short.clone(), item: Some(it), count, included: true })
        })
        .collect();
    rows.push(Detected { item: None, label: "FrAzenV2".into(), count: 1, included: false });
    rows
}

/// Открывает оверлей поверх всех окон.
pub fn open(rows: Vec<Detected>, mode: String, log: Entity<RaidLog>, sample: bool, cx: &mut App) {
    let bounds = Bounds::centered(
        None,
        size(px(560.0 + OVERLAY_MARGIN * 2.0), px(720.0 + OVERLAY_MARGIN * 2.0)),
        cx,
    );
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            kind: WindowKind::PopUp,
            focus: true,
            is_resizable: false,
            is_minimizable: false,
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        |_, cx| {
            cx.new(|_| RaidOverlay {
                rows,
                map: None,
                mode,
                log,
                sample,
                #[cfg(windows)]
                outside: None,
            })
        },
    );
    if let Ok(handle) = handle {
        let _ = handle.update(cx, |_, window, _| keep_visible(window));
        // Windows: PopUp у GPUI не «поверх всех» — без этого окно уйдёт под игру.
        // Win32-вызов — вне обновления окна, в следующем такте.
        #[cfg(windows)]
        if let Ok(Some(hwnd)) = handle.update(cx, |_, window, _| crate::scanner::win::window_hwnd(window)) {
            cx.spawn(async move |_| crate::scanner::win::set_topmost(hwnd)).detach();
            close_on_outside_click(handle, hwnd, cx);
        }
        // без хука мыши клик мимо виден только как потеря фокуса
        #[cfg(not(windows))]
        let _ = handle.update(cx, |_, window, cx| {
            let mut was_active = window.is_window_active();
            cx.observe_window_activation(window, move |_, window, _| {
                let active = window.is_window_active();
                if was_active && !active {
                    window.remove_window();
                }
                was_active = active;
            })
            .detach();
        });
    }
}

/// Клик мимо панели закрывает окно, как «Отмена».
#[cfg(windows)]
fn close_on_outside_click(handle: gpui::WindowHandle<RaidOverlay>, hwnd: isize, cx: &mut App) {
    use futures::StreamExt;
    let (watch, mut clicks) = crate::scanner::win::watch_outside_clicks(hwnd, OVERLAY_MARGIN);
    let _ = handle.update(cx, |this, _, _| this.outside = Some(watch));
    cx.spawn(async move |cx| {
        // окно закрыли кнопкой — слежка снята, канал закрылся
        if clicks.next().await.is_some() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    })
    .detach();
}

/// Нативная донастройка окна оверлея. На macOS PopUp — это NSPanel: панели по умолчанию
/// прячутся при деактивации (а активна будет игра), и у них остаётся спрятанный заголовок.
#[allow(unexpected_cfgs)] // макросы objc 0.2 проверяют cfg cargo-clippy
fn keep_visible(window: &Window) {
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::{Object, NO};
        use objc::{msg_send, sel, sel_impl};
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window) {
            if let RawWindowHandle::AppKit(appkit) = handle.as_raw() {
                let view = appkit.ns_view.as_ptr() as *mut Object;
                unsafe {
                    let ns_window: *mut Object = msg_send![view, window];
                    if !ns_window.is_null() {
                        let _: () = msg_send![ns_window, setHidesOnDeactivate: NO];
                        // у «окна без заголовка» GPUI заголовок просто спрятан — убираем его
                        // разделитель и подсветку, иначе сверху двоится рамка
                        let _: () = msg_send![ns_window, setTitlebarAppearsTransparent: objc::runtime::YES];
                        let _: () = msg_send![ns_window, setTitleVisibility: 1i64]; // NSWindowTitleHidden
                        let _: () = msg_send![ns_window, setTitlebarSeparatorStyle: 1i64]; // None
                        let _: () = msg_send![ns_window, setOpaque: NO];
                        let clear: *mut Object = msg_send![objc::class!(NSColor), clearColor];
                        let _: () = msg_send![ns_window, setBackgroundColor: clear];
                        // системная тень рисует светлый контур по краю окна — вторая линия
                        // над нашей рамкой; тень рисуем сами вокруг панели
                        let _: () = msg_send![ns_window, setHasShadow: NO];
                    }
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window; // Windows: «поверх всех окон» ставит `open` через scanner::win::set_topmost
}

impl RaidOverlay {
    fn total(&self) -> i64 {
        self.rows
            .iter()
            .filter(|r| r.included)
            .filter_map(|r| r.item.as_ref().map(|i| i.price() * r.count as i64))
            .sum()
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(map) = self.map else { return };
        let items = self
            .rows
            .iter()
            .filter(|r| r.included)
            .filter_map(|r| {
                r.item.as_ref().map(|i| RaidItem { id: i.id.clone(), short: i.short.clone(), count: r.count, price: i.price() })
            })
            .collect();
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let raid = Raid { ts, map: map.to_string(), mode: self.mode.clone(), items };
        self.log.update(cx, |log, cx| log.add(raid, cx));
        window.remove_window();
    }
}

impl Render for RaidOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.total();
        let included = self.rows.iter().filter(|r| r.included && r.item.is_some()).count();
        let can_confirm = self.map.is_some();

        let rows = self.rows.iter().enumerate().map(|(i, r)| {
            let on = r.included && r.item.is_some();
            let (name, full, price) = match &r.item {
                Some(it) => (it.short.clone(), it.name.clone(), Some(it.price() * r.count as i64)),
                None => (r.label.clone(), "не опознан — не считается".to_string(), None),
            };
            div()
                .id(ElementId::named_usize("row", i))
                .flex()
                .items_center()
                .gap_3()
                .px_2()
                .py(px(5.0))
                .when(r.item.is_some(), |el| {
                    el.cursor_pointer().hover(|s| s.bg(rgb(CARD))).on_click(cx.listener(move |this, _, _, cx| {
                        this.rows[i].included = !this.rows[i].included;
                        cx.notify();
                    }))
                })
                .child(checkbox(if on { Check::On } else { Check::Off }))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(if on { TEXT } else { MUTED }))
                                .when(!on, |el| el.line_through())
                                .child(if r.count > 1 { format!("{name} ×{}", r.count) } else { name }),
                        )
                        .child(div().text_xs().text_color(rgb(MUTED)).child(full)),
                )
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(match price {
                            Some(p) if on => tier_color(p),
                            _ => MUTED,
                        }))
                        .child(price.map(fmt_short).unwrap_or_else(|| "—".into())),
                )
        });

        let maps = MAPS.iter().enumerate().map(|(i, m)| {
            let id = m.id;
            segmented(
                ElementId::named_usize("map", i),
                m.title(),
                self.map == Some(id),
                cx.listener(move |this, _, _, cx| {
                    this.map = Some(id);
                    cx.notify();
                }),
            )
        });

        let button = |id: &'static str, label: SharedString, primary: bool, enabled: bool| {
            div()
                .id(id)
                .px_4()
                .py_2()
                .border_1()
                .border_color(rgb(if primary && enabled { GOLD } else { GOLD_DIM }))
                .bg(rgb(if primary && enabled { GOLD } else { PANEL }))
                .text_color(rgb(if primary && enabled {
                    0x1a1a1a
                } else if enabled {
                    TEXT
                } else {
                    MUTED
                }))
                .font_weight(FontWeight::BOLD)
                .when(enabled, |el| el.cursor_pointer().hover(|s| s.opacity(0.9)))
                .child(label)
        };

        // окно прозрачное, панель со скруглением и рамкой рисуем сами с отступом —
        // иначе система обрезает рамку по краям окна без заголовка
        div().size_full().p(px(OVERLAY_MARGIN)).child(
        div()
            .size_full()
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .shadow_lg()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .border(px(1.5))
            .border_color(rgb(GOLD))
            .text_color(rgb(TEXT))
            .font_family(if cfg!(target_os = "windows") { "Segoe UI" } else { ".SystemUIFont" })
            // шапка
            .child(
                div()
                    .px_4()
                    .pt_4()
                    .pb_3()
                    .border_b_1()
                    .border_color(rgb(GOLD_DIM))
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            // подсказка переносится, а не выталкивает крестик
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_lg().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child("ПОСЛЕ РЕЙДА"))
                            .child(div().text_xs().text_color(rgb(MUTED)).child(if self.sample {
                                "Пример с твоего скрина рюкзака. Сними галочки с того, что не надо считать. Клик мимо окна — отмена."
                            } else {
                                "Сними галочки с того, что не надо считать (своё снаряжение и т. п.). Клик мимо окна — отмена."
                            })),
                    )
                    .child(
                        div()
                            .id("close")
                            .text_color(rgb(MUTED))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .child("✕")
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            // предметы
            .child(div().id("rows").flex_1().overflow_y_scroll().px_2().py_2().children(rows))
            // итог, карта, кнопки
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(GOLD_DIM))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_end()
                            .child(div().text_sm().text_color(rgb(MUTED)).child(format!("Считаю {included} предм.")))
                            .child(
                                div()
                                    .flex()
                                    .items_end()
                                    .gap_2()
                                    .child(div().text_sm().text_color(rgb(MUTED)).child("ИТОГО"))
                                    .child(div().text_xl().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child(fmt_rub(total))),
                            ),
                    )
                    .child(div().text_sm().text_color(rgb(GOLD)).child("НА КАКОЙ КАРТЕ БЫЛ РЕЙД"))
                    .child(div().flex().flex_wrap().gap_1().children(maps))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                button("cancel", "Отмена".into(), false, true).on_click(|_, window, _| window.remove_window()),
                            )
                            .child(
                                button(
                                    "confirm",
                                    if can_confirm { "Подтвердить".into() } else { "Выбери карту".into() },
                                    true,
                                    can_confirm,
                                )
                                .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx))),
                            ),
                    ),
            )
        )
    }
}
