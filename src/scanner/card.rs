//! Карточка цены: одна и та же в превью на странице сканера и в окне поверх игры.
//! Высоты строк фиксированы — окно карточки подгоняется под содержимое заранее.

use gpui::{div, prelude::*, px, rgb, Context, Div, FontWeight, Window};

use crate::app::{tier_color, with_alpha, GOLD, MUTED, TEXT};
use crate::data::{fmt_rub, Item, Need};
use crate::settings::ScannerSettings;

pub const CARD_W: f32 = 280.0;
const PAD: f32 = 12.0;
const TITLE_H: f32 = 24.0;
const SUB_H: f32 = 18.0;
const ROW_H: f32 = 22.0;
const NEED_HEAD_H: f32 = 22.0;
const NEED_H: f32 = 18.0;
const FOOT_H: f32 = 26.0;
const BORDER: f32 = 1.0;
/// Сколько требований показываем, остальные — «и ещё N».
const MAX_NEEDS: usize = 4;

fn frame() -> Div {
    div()
        .w(px(CARD_W))
        .flex()
        .flex_col()
        .p(px(PAD))
        .bg(with_alpha(0x15140f, 0xf2))
        .border(px(BORDER))
        .border_color(rgb(GOLD))
        .text_color(rgb(TEXT))
}

fn line(h: f32) -> Div {
    div().h(px(h)).flex().items_center().overflow_hidden()
}

fn value_row(label: String, value: String, color: u32) -> Div {
    line(ROW_H)
        .justify_between()
        .gap_4()
        .text_sm()
        .child(div().text_color(rgb(MUTED)).child(label))
        .child(div().font_weight(FontWeight::BOLD).text_color(rgb(color)).child(value))
}

fn footer(text: &str) -> Div {
    line(FOOT_H).items_end().text_xs().text_color(rgb(MUTED)).child(text.to_string())
}

/// Карточка предмета и её высота в логических пикселях.
pub fn price_card(item: &Item, needs: &[Need], others: usize, sc: &ScannerSettings, foot: &str) -> (Div, f32) {
    let mut h = PAD * 2.0 + BORDER * 2.0 + TITLE_H + SUB_H + FOOT_H;
    let mut card = frame()
        .child(line(TITLE_H).text_lg().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child(item.short.clone()))
        .child(line(SUB_H).text_xs().text_color(rgb(MUTED)).child(item.name.clone()));
    if others > 0 {
        h += SUB_H;
        card = card.child(
            line(SUB_H).text_xs().text_color(rgb(MUTED)).child(format!("такой же ярлык ещё у {others} — показан самый дорогой")),
        );
    }
    let per_slot = item.price() / item.slots() as i64;
    if sc.show_per_slot {
        h += ROW_H;
        card = card.child(value_row(format!("За клетку ({}×{})", item.width, item.height), fmt_rub(per_slot), tier_color(per_slot)));
    }
    if sc.show_flea {
        h += ROW_H;
        let v = if item.flea > 0 { fmt_rub(item.flea) } else { "нельзя продать".into() };
        card = card.child(value_row("Барахолка".into(), v, TEXT));
    }
    if sc.show_trader && item.trader > 0 {
        h += ROW_H;
        card = card.child(value_row(item.trader_name.to_string(), fmt_rub(item.trader), TEXT));
    }
    if sc.show_needed {
        if needs.is_empty() {
            h += NEED_H;
            card = card.child(line(NEED_H).text_xs().text_color(rgb(MUTED)).child("Не нужен для убежища и задач"));
        } else {
            h += NEED_HEAD_H;
            card = card.child(line(NEED_HEAD_H).items_end().text_xs().text_color(rgb(GOLD)).child("НУЖЕН"));
            for n in needs.iter().take(MAX_NEEDS) {
                h += NEED_H;
                card = card.child(
                    line(NEED_H)
                        .gap_2()
                        .text_xs()
                        .child(div().child(if n.hideout { "🏠" } else { "📜" }))
                        .child(div().flex_1().overflow_hidden().text_color(rgb(TEXT)).child(n.name.clone()))
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(MUTED))
                                .child(format!("×{}{}", n.count, if n.found_in_raid { " · FIR" } else { "" })),
                        ),
                );
            }
            if needs.len() > MAX_NEEDS {
                h += NEED_H;
                card = card.child(
                    line(NEED_H).text_xs().text_color(rgb(MUTED)).child(format!("и ещё {}", needs.len() - MAX_NEEDS)),
                );
            }
        }
    }
    if !(sc.show_per_slot || sc.show_flea || sc.show_trader || sc.show_needed) {
        h += ROW_H;
        card = card.child(line(ROW_H).text_sm().text_color(rgb(MUTED)).child("Ничего не выбрано для показа"));
    }
    (card.child(footer(foot)), h)
}

/// Карточка «не узнал»: что прочитали и почему могло не выйти.
pub fn missing_card(read: &str, foot: &str) -> (Div, f32) {
    let hint = if read.is_empty() {
        "Наведи курсор на предмет в инвентаре или тайнике".to_string()
    } else {
        format!("Прочитано «{read}» — такого ярлыка нет в списке")
    };
    let h = PAD * 2.0 + BORDER * 2.0 + TITLE_H + SUB_H * 2.0 + FOOT_H;
    let card = frame()
        .child(line(TITLE_H).text_lg().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child("Не узнал предмет"))
        .child(div().h(px(SUB_H * 2.0)).text_xs().text_color(rgb(MUTED)).child(hint))
        .child(footer(foot));
    (card, h)
}

/// Что показывает карточка поверх игры.
pub enum CardContent {
    Found { item: Item, needs: Vec<Need>, others: usize },
    Missing { read: String },
}

/// Окно карточки поверх игры.
pub struct ScanCard {
    pub content: CardContent,
    pub settings: ScannerSettings,
    pub footer: String,
}

impl ScanCard {
    pub fn height(&self) -> f32 {
        self.element().1
    }

    fn element(&self) -> (Div, f32) {
        match &self.content {
            CardContent::Found { item, needs, others } => price_card(item, needs, *others, &self.settings, &self.footer),
            CardContent::Missing { read } => missing_card(read, &self.footer),
        }
    }
}

impl Render for ScanCard {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.element().0.size_full())
    }
}

/// Подпись внизу карточки: как она скроется.
pub fn footer_text(sc: &ScannerSettings) -> String {
    let keys = |k: &str| crate::hotkey::display(k).join(" + ");
    if sc.hold_to_show {
        format!("пока держишь {}", keys(&sc.scan_key))
    } else if sc.show_seconds == 0 {
        format!("{} — скрыть", keys(&sc.hide_key))
    } else {
        format!("скроется через {} с · {} — скрыть", sc.show_seconds, keys(&sc.hide_key))
    }
}
