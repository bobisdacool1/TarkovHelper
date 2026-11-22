//! Раздел «Задания»: список заданий карты, как их выполнить и точки целей на карте.

use super::*;

impl HelperApp {
    /// Задание проходит поиск: все слова — в названии, торговце или тексте целей.
    fn task_matches(&self, t: &Task) -> bool {
        let q = data::normalize(self.task_search.trim());
        if q.is_empty() {
            return true;
        }
        let hay = data::normalize(&format!(
            "{}|{}|{}",
            t.name,
            t.trader,
            t.objectives.iter().map(|o| o.text.as_str()).collect::<Vec<_>>().join("|")
        ));
        q.split_whitespace().all(|w| hay.contains(w))
    }

    /// Карта в разделе заданий: с подложкой — переключаем и карту, без неё — только список.
    fn set_tasks_map(&mut self, map: &'static str, cx: &mut Context<Self>) {
        self.selected_task = None;
        self.hover = None;
        if crate::maps::get(map).id == map {
            self.set_map(map, cx);
        } else {
            self.tasks_map = map;
        }
        cx.notify();
    }

    /// Открывает задание и подводит карту к его целям.
    pub(super) fn select_task(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.hover = None;
        self.selected_task = id;
        let pts: Vec<(usize, f64, f64)> = match (&self.data, &self.selected_task) {
            (Some(d), Some(id)) if self.tasks_map == self.map => d
                .tasks
                .iter()
                .find(|t| &t.id == id)
                .map(|t| {
                    t.points_on(self.map)
                        .map(|(_, p)| (crate::maps::floor_of(self.map, p.pos), p.pos.x, p.pos.z))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        self.focus_points(&pts);
        cx.notify();
    }

    /// Зум и сдвиг так, чтобы точки (этаж, x, z) были в центре окна карты.
    fn focus_points(&mut self, pts: &[(usize, f64, f64)]) {
        let bounds = *self.map_bounds.borrow();
        let (Some(b), Some(geo), false) = (bounds, self.geo, pts.is_empty()) else { return };
        // открыт один этаж, а целей на нём нет — переходим на этаж первой цели
        if let Some(f) = self.floor_view {
            if !pts.iter().any(|p| p.0 == f) {
                self.floor_view = Some(pts[0].0);
            }
        }
        let world: Vec<(f32, f32)> = pts
            .iter()
            .filter(|p| self.floor_visible(p.0))
            .map(|&(f, x, z)| {
                let (px_, py_) = geo.project(x, z);
                (self.floor_origin(f) + px_, py_)
            })
            .collect();
        if world.is_empty() {
            return;
        }
        let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for (x, y) in &world {
            x0 = x0.min(*x);
            x1 = x1.max(*x);
            y0 = y0.min(*y);
            y1 = y1.max(*y);
        }
        let (vw, vh) = (f32::from(b.size.width), f32::from(b.size.height) - 30.0);
        let scale = ((vw * 0.6) / (x1 - x0).max(1.0)).min((vh * 0.6) / (y1 - y0).max(1.0));
        self.scale = scale.clamp(0.1, 1.2);
        let (cxw, cyw) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        self.offset = (vw / 2.0 - cxw * self.scale, 30.0 + vh / 2.0 - cyw * self.scale);
        self.fitted = true;
    }

    /// Точки и зоны заданий на карте: открытое задание — цели с номерами, иначе все задания карты.
    pub(super) fn task_layers(&self, d: &MapData, layers: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let map = self.map;
        let floors = self.floors();
        let sel = self.selected_task.clone();
        let shown: Vec<&Task> = d
            .tasks
            .iter()
            .filter(|t| match &sel {
                Some(id) => &t.id == id,
                None => t.touches(map) && self.task_matches(t),
            })
            .collect();

        // контуры зон
        let mut polys: Vec<(Vec<Point<Pixels>>, u32)> = Vec::new();
        for t in &shown {
            for (oi, p) in t.points_on(map) {
                let floor = crate::maps::floor_of(map, p.pos);
                if !self.floor_visible(floor) || p.outline.len() < 3 {
                    continue;
                }
                let color = if sel.is_some() { obj_color(oi) } else { GOLD };
                let pts = p
                    .outline
                    .iter()
                    .map(|q| {
                        let (x, y) = self.to_screen(floor, q[0], q[1]);
                        point(px(x), px(y))
                    })
                    .collect();
                polys.push((pts, color));
            }
        }
        layers.push(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let o = bounds.origin;
                    for (pts, color) in &polys {
                        let mut fill = PathBuilder::fill();
                        let mut line = PathBuilder::stroke(px(2.0));
                        fill.move_to(pts[0] + o);
                        line.move_to(pts[0] + o);
                        for p in &pts[1..] {
                            fill.line_to(*p + o);
                            line.line_to(*p + o);
                        }
                        fill.close();
                        line.close();
                        if let Ok(path) = fill.build() {
                            window.paint_path(path, Hsla::from(with_alpha(*color, 0x30)));
                        }
                        if let Ok(path) = line.build() {
                            window.paint_path(path, Hsla::from(with_alpha(*color, 0xe6)));
                        }
                    }
                },
            )
            .absolute()
            .size_full()
            .into_any_element(),
        );

        // маркеры целей
        let mut n = 0usize;
        for t in &shown {
            for (oi, p) in t.points_on(map) {
                let floor = crate::maps::floor_of(map, p.pos);
                if !self.floor_visible(floor) {
                    continue;
                }
                n += 1;
                let (sx, sy) = self.to_screen(floor, p.pos.x, p.pos.z);
                let o = &t.objectives[oi];
                let mut lines = vec![format!("📜 {}", t.name), format!("{}. {}", oi + 1, o.text)];
                lines.extend(o.details.iter().cloned());
                if !floors[floor].is_empty() {
                    lines.push(floors[floor].to_string());
                }
                if sel.is_none() {
                    lines.push("клик — как выполнить".into());
                }
                let (label, color) = if sel.is_some() { ((oi + 1).to_string(), obj_color(oi)) } else { ("!".to_string(), GOLD) };
                let id = t.id.clone();
                let key = format!("task{n}");
                layers.push(
                    div()
                        .id(ElementId::named_usize("task-pt", n))
                        .absolute()
                        .left(px(sx - 11.0))
                        .top(px(sy - 11.0))
                        .size(px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgb(color))
                        .border_2()
                        .border_color(rgb(0x111111))
                        .shadow_md()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(0x111111))
                        .child(label)
                        .cursor_pointer()
                        .on_hover(Self::hover_listener(key, lines, floor, p.pos.x, p.pos.z, cx))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.select_task(Some(id.clone()), cx);
                            cx.stop_propagation();
                        }))
                        .into_any_element(),
                );
            }
        }
    }

    /// Левая панель раздела: карта, поиск, список или открытое задание.
    pub(super) fn render_tasks_sidebar(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let map = self.tasks_map;
        let focused = self.task_search_input.focus_handle(cx).is_focused(window);
        let empty = self.task_search.is_empty();

        let chips = div().mt_3().flex().flex_wrap().gap_1().children(crate::maps::MAPS.iter().enumerate().map(|(i, m)| {
            let id = m.id;
            segmented(ElementId::named_usize("task-map", i), m.title(), m.id == map, cx.listener(move |this, _, _, cx| {
                this.set_tasks_map(id, cx);
            }))
        }));

        let search = div()
            .mt_3()
            .flex()
            .h(px(40.0))
            .border_1()
            .border_color(rgb(if focused { GOLD } else { GOLD_DIM }))
            .child(div().flex_1().flex().items_center().px_3().overflow_hidden().text_color(rgb(TEXT)).child(self.task_search_input.clone()))
            .child(
                div()
                    .id("task-search-clear")
                    .w(px(40.0))
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
                        this.task_search_input.update(cx, |input, cx| input.clear(cx));
                    })),
            );

        let content: AnyElement = match self.data.clone() {
            None => div()
                .mt_4()
                .text_sm()
                .text_color(rgb(MUTED))
                .child(match &self.status {
                    Status::Error(e) => format!("Не удалось загрузить данные: {e}"),
                    _ => "Загрузка…".into(),
                })
                .into_any_element(),
            Some(d) => match self.selected_task.as_ref().and_then(|id| d.tasks.iter().find(|t| &t.id == id)) {
                Some(t) => self.render_task_detail(t, cx).into_any_element(),
                None => self.render_task_list(&d, cx).into_any_element(),
            },
        };

        div()
            .id("tasks-sidebar")
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
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child("ЗАДАНИЯ"))
                    .child(div().text_xs().text_color(rgb(MUTED)).child(self.mode.label())),
            )
            .child(chips)
            .when(self.selected_task.is_none(), |el| el.child(search))
            .child(content)
    }

    fn render_task_list(&self, d: &MapData, cx: &mut Context<Self>) -> impl IntoElement {
        let map = self.tasks_map;
        let mut local: Vec<&Task> = Vec::new();
        let mut shared: Vec<&Task> = Vec::new();
        for t in d.tasks.iter().filter(|t| t.touches(map) && self.task_matches(t)) {
            if t.is_local(map) { local.push(t) } else { shared.push(t) }
        }
        for list in [&mut local, &mut shared] {
            list.sort_by(|a, b| a.level.cmp(&b.level).then(a.trader.cmp(b.trader)).then(a.name.cmp(&b.name)));
        }
        let heading = |t: String| div().mt_5().mb_1().text_sm().text_color(rgb(GOLD)).child(t);
        let mut col = div().flex().flex_col();
        if local.is_empty() && shared.is_empty() {
            col = col.child(div().mt_4().text_sm().text_color(rgb(MUTED)).child("Заданий не найдено."));
        }
        if !local.is_empty() {
            col = col
                .child(heading(format!("ЗАДАНИЯ ЛОКАЦИИ · {}", local.len())))
                .children(local.iter().map(|t| self.task_row(t, map, cx)));
        }
        if !shared.is_empty() {
            col = col
                .child(heading(format!("МОЖНО ВЫПОЛНИТЬ И ЗДЕСЬ · {}", shared.len())))
                .child(div().mb_1().text_xs().text_color(rgb(MUTED)).child("Цели на нескольких картах: убийства, выходы, предметы."))
                .children(shared.iter().map(|t| self.task_row(t, map, cx)));
        }
        col
    }

    fn task_row(&self, t: &Task, map: &str, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id.clone();
        let points = t.points_on(map).count();
        let mut meta = vec![t.trader.to_string()];
        if t.level > 0 {
            meta.push(format!("ур. {}", t.level));
        }
        if points > 0 {
            meta.push(format!("📍 {points}"));
        }
        if let Some(f) = &t.faction {
            meta.push(format!("только {f}"));
        }
        div()
            .id(ElementId::Name(format!("task-{}", t.id).into()))
            .flex()
            .flex_col()
            .py(px(6.0))
            .px_1()
            .border_b_1()
            .border_color(rgb(0x2a2824))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(CARD)))
            .on_click(cx.listener(move |this, _, _, cx| this.select_task(Some(id.clone()), cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child(t.name.clone()))
                    .when(t.kappa, |el| el.child(badge("КАППА", 0xc2ab74)))
                    .when(t.lightkeeper, |el| el.child(badge("СМОТРИТЕЛЬ", 0x7e57c2))),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(meta.join(" · ")))
            .into_any_element()
    }

    fn render_task_detail(&self, t: &Task, cx: &mut Context<Self>) -> impl IntoElement {
        let map = self.tasks_map;
        let has_map = crate::maps::get(map).id == map;
        let mut meta = vec![t.trader.to_string()];
        if t.level > 0 {
            meta.push(format!("с {} уровня", t.level));
        }
        if let Some(f) = &t.faction {
            meta.push(format!("только {f}"));
        }
        let wiki = t.wiki.clone();

        let objectives = t.objectives.iter().enumerate().map(|(i, o)| {
            let here = o.points.iter().filter(|p| p.map == map).count();
            let mut place: Option<String> = None;
            if o.maps.is_empty() {
                place = Some("у торговца".into());
            } else if !o.maps.iter().any(|m| m == map) {
                let names: Vec<&str> = o.maps.iter().map(|m| crate::maps::name(m)).filter(|n| *n != "?").collect();
                place = Some(format!("на карте: {}", if names.is_empty() { "другой".into() } else { names.join(", ") }));
            } else if o.maps.len() > 1 {
                let names: Vec<&str> = o.maps.iter().map(|m| crate::maps::name(m)).filter(|n| *n != "?").collect();
                place = Some(format!("карты: {}", names.join(", ")));
            }
            let color = obj_color(i);
            div()
                .mt_2()
                .flex()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .mt(px(1.0))
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(if here > 0 { rgb(color) } else { rgb(CARD) })
                        .border_1()
                        .border_color(rgb(if here > 0 { 0x111111 } else { GOLD_DIM }))
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(if here > 0 { 0x111111 } else { MUTED }))
                        .child((i + 1).to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(div().text_sm().child(o.text.clone()))
                        .children(o.details.iter().map(|l| div().text_xs().text_color(rgb(MUTED)).child(l.clone())))
                        .when_some(place, |el, p| el.child(div().text_xs().text_color(rgb(MUTED)).child(p)))
                        .when(here > 0, |el| {
                            el.child(div().text_xs().text_color(rgb(color)).child(if has_map {
                                format!("📍 на карте: {here}")
                            } else {
                                format!("📍 точек: {here} (карты нет в приложении)")
                            }))
                        })
                        .when(o.optional, |el| el.child(div().text_xs().text_color(rgb(GOLD_DIM)).child("необязательно"))),
                )
        });

        div()
            .mt_4()
            .flex()
            .flex_col()
            .child(link("task-back", "← Все задания", cx.listener(|this, _, _, cx| {
                this.selected_task = None;
                this.hover = None;
                cx.notify();
            })))
            .child(div().mt_3().text_lg().font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child(t.name.clone()))
            .child(
                div()
                    .mt_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_sm().text_color(rgb(MUTED)).child(meta.join(" · ")))
                    .when(t.kappa, |el| el.child(badge("КАППА", 0xc2ab74)))
                    .when(t.lightkeeper, |el| el.child(badge("СМОТРИТЕЛЬ", 0x7e57c2))),
            )
            .when(!t.requires.is_empty(), |el| {
                el.child(div().mt_2().text_xs().text_color(rgb(MUTED)).child(format!("Сначала: {}", t.requires.join(", "))))
            })
            .child(div().mt_5().text_sm().text_color(rgb(GOLD)).child("КАК ВЫПОЛНИТЬ"))
            .children(objectives)
            .when(!t.keys.is_empty(), |el| {
                el.child(div().mt_5().text_sm().text_color(rgb(GOLD)).child("КЛЮЧИ"))
                    .children(t.keys.iter().map(|(m, names)| {
                        div().mt_1().text_sm().child(format!("🔑 {} — {}", names.join(" или "), crate::maps::name(m)))
                    }))
            })
            .when(!wiki.is_empty(), |el| {
                el.child(
                    div()
                        .id("task-wiki")
                        .mt_5()
                        .px_3()
                        .py_2()
                        .border_1()
                        .border_color(rgb(GOLD_DIM))
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .cursor_pointer()
                        .hover(|s| s.border_color(rgb(GOLD)).text_color(rgb(GOLD)))
                        .child("Подробный гайд в вики ↗")
                        .on_click(move |_, _, cx| cx.open_url(&wiki)),
                )
            })
            .when(has_map && t.points_on(map).next().is_none(), |el| {
                el.child(div().mt_3().text_xs().text_color(rgb(MUTED)).child("Точных мест у этого задания в данных нет — смотри текст целей."))
            })
    }
}

fn badge(text: &'static str, color: u32) -> impl IntoElement {
    div()
        .flex_none()
        .px_1()
        .text_size(px(10.0))
        .font_weight(FontWeight::BOLD)
        .border_1()
        .border_color(rgb(color))
        .text_color(rgb(color))
        .child(text)
}

/// Вместо карты, если подложки этой локации в приложении ещё нет.
pub(super) fn no_map_placeholder(map: &str) -> impl IntoElement {
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .bg(rgb(BG))
        .child(div().text_xl().text_color(rgb(GOLD)).child(crate::maps::name(map).to_uppercase()))
        .child(div().text_sm().text_color(rgb(MUTED)).child("Карты этой локации в приложении пока нет — задания и цели списком слева."))
}
