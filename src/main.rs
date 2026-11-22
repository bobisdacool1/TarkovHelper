mod app;
mod categories;
mod data;
mod hotkey;
mod labs;
mod maps;
mod raid_overlay;
mod raids;
mod scanner;
mod settings;
mod tasks;
mod text_input;
mod tiles;
mod updater;

use gpui::{
    actions, prelude::*, px, size, App, Application, Bounds, KeyBinding, TitlebarOptions, WindowBounds,
    WindowOptions,
};

actions!(tarkov_helper, [Quit]);

/// SVG-иконки, вшитые в бинарник.
struct Assets;

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/lock.svg" => include_bytes!("../assets/icons/lock.svg"),
            "icons/exit.svg" => include_bytes!("../assets/icons/exit.svg"),
            "icons/transit.svg" => include_bytes!("../assets/icons/transit.svg"),
            "icons/stairs.svg" => include_bytes!("../assets/icons/stairs.svg"),
            "icons/spawn.svg" => include_bytes!("../assets/icons/spawn.svg"),
            _ => return Ok(None),
        };
        Ok(Some(std::borrow::Cow::Borrowed(bytes)))
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<gpui::SharedString>> {
        Ok(Vec::new())
    }
}

fn main() {
    // сканеру нужны физические пиксели: снимок экрана, курсор и окна поверх игры в одних координатах
    #[cfg(windows)]
    scanner::win::make_dpi_aware();
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.bind_keys(text_input::key_bindings());
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1500.0), px(950.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Tarkov Helper".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(app::HelperApp::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
