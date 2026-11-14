mod http;
mod lib_index;
mod net;
mod player;
mod session;
mod store;
mod ui;
mod yandex;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

use crate::store::Store;

fn main() {
    gpui_kit::application()
        .with_assets(ui::Icons)
        .run(|cx| {
            gpui_kit::init(cx);

            cx.set_http_client(crate::http::http_client());

            let store = cx.new(|_| Store::open());
            let settings = store.read(cx).settings.clone();

            let mode = if settings.theme_dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            };
            Theme::change(mode, None, cx);
            if let Some(hex) = settings.accent.clone() {
                if let Some(color) = ui::parse_hex(&hex) {
                    Theme::update(cx, |theme| theme.primary = color);
                }
            }

            let mut options = gpui_kit::WindowOptions::default();
            options.titlebar = Some(TitlebarOptions {
                title: Some("meowsic".into()),
                ..Default::default()
            });

            let (_window, _view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::MusicApp::new(window, store.clone(), cx))
            })
            .expect("не удалось открыть окно");

        });
}

