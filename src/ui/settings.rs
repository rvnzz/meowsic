use gpui_kit::component::{
    ActiveTheme, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputContentType, InputState},
    scroll::ScrollableElement,
    slider::{Slider, SliderEvent, SliderState},
    switch::Switch,
    Selectable,
    h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::PageCtx;
use crate::player::Player;
use std::rc::Rc;
use crate::session::SessionEvent;
use crate::store::{EQ_BANDS, EQ_FREQUENCIES, Quality};
use crate::ui::widgets::{SettingRow, StatusBanner};

pub struct SettingsPage {
    ctx: PageCtx,
    settings: crate::store::Settings,
    token_input: Entity<InputState>,
    accent_input: Entity<InputState>,
    login_error: Option<SharedString>,
    eq_sliders: Vec<Entity<SliderState>>,
    _subs: Vec<Subscription>,
}

const EQ_PRESETS: [(&str, [f32; EQ_BANDS]); 5] = [
    ("Обычный", [0.; 8]),
    ("Бас", [6., 5., 3., 0., -1., -1., 0., 0.]),
    ("Вокал", [-2., -1., 0., 3., 4., 3., 0., -1.]),
    ("Электроника", [5., 4., 0., -2., 0., 3., 4., 5.]),
    ("Рок", [4., 3., -1., -1., 1., 3., 3., 4.]),
];

impl SettingsPage {
    pub fn new(
        ctx: PageCtx,
        settings: crate::store::Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let token_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("OAuth-токен Яндекса")
                .default_value(settings.token.clone())
        });
        let accent_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("#FF0099")
                .default_value(settings.accent.clone().unwrap_or_default())
        });

        let mut eq_sliders = Vec::new();
        for ix in 0..EQ_BANDS {
            let value = settings.eq.gains_db.get(ix).copied().unwrap_or(0.0);
            eq_sliders.push(cx.new(|_| SliderState::new().min(-12.0).max(12.0).default_value(value)));
        }

        let mut this = Self {
            ctx,
            settings: settings.clone(),
            token_input,
            accent_input,
            login_error: None,
            eq_sliders,
            _subs: Vec::new(),
        };
        let session = this.ctx.session.clone();
        this._subs.push(cx.subscribe(&session, |this, _, event, cx| {
            match event {
                SessionEvent::LoggedIn => {
                    this.login_error = None;
                    cx.notify();
                }
                SessionEvent::LoginFailed(_) | SessionEvent::LoggedOut => cx.notify(),
            }
        }));
        for state in this.eq_sliders.clone() {
            this._subs.push(cx.subscribe(&state, |this, _, event, cx| {
                if matches!(event, SliderEvent::Release(_)) {
                    this.commit_eq(cx);
                }
            }));
        }
        this
    }

    pub fn set_settings(&mut self, settings: crate::store::Settings, cx: &mut Context<Self>) {
        self.settings = settings;
        cx.notify();
    }

    pub fn set_login_error(&mut self, msg: SharedString, cx: &mut Context<Self>) {
        self.login_error = Some(msg);
        cx.notify();
    }

    fn commit_eq(&mut self, cx: &mut Context<Self>) {
        let gains: Vec<f32> = self
            .eq_sliders
            .iter()
            .map(|s| s.read(cx).value().start())
            .collect();
        let app = self.ctx.app.clone();
        let _ = app.update(cx, |app, cx| {
            app.update_settings(
                |s| {
                    s.eq.gains_db = gains;
                    s.eq.preset = "custom".to_string();
                },
                cx,
            );
        });
    }

    fn apply_preset(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (name, gains) = EQ_PRESETS[ix];
        for (ix, state) in self.eq_sliders.iter().enumerate() {
            let g = gains[ix];
            state.update(cx, |s, cx| s.set_value(g, window, cx));
        }
        let app = self.ctx.app.clone();
        let gains = gains.to_vec();
        let name = name.to_string();
        let _ = app.update(cx, |app, cx| {
            app.update_settings(
                |s| {
                    s.eq.gains_db = gains;
                    s.eq.preset = name;
                },
                cx,
            );
        });
    }

    fn set_quality(&mut self, q: Quality, cx: &mut Context<Self>) {
        let app = self.ctx.app.clone();
        let _ = app.update(cx, |app, cx| {
            app.update_settings(|s| s.quality = q, cx);
        });
    }

    fn set_theme(&mut self, dark: bool, cx: &mut Context<Self>) {
        let app = self.ctx.app.clone();
        let _ = app.update(cx, |app, cx| {
            app.update_settings(|s| s.theme_dark = dark, cx);
            app.apply_theme(cx);
        });
    }

    fn set_flag(&mut self, which: Flag, value: bool, cx: &mut Context<Self>) {
        let app = self.ctx.app.clone();
        let _ = app.update(cx, |app, cx| {
            app.update_settings(
                |s| match which {
                    Flag::Autoplay => s.autoplay_restored = value,
                    Flag::Restore => s.restore_session = value,
                    Flag::Covers => s.sidebar_covers = value,
                },
                cx,
            );
        });
    }

    fn start_login(&mut self, cx: &mut Context<Self>) {
        let value = self.token_input.read(cx).value().to_string();
        self.ctx.session.update(cx, |s, cx| s.login(value, cx));
    }

    fn do_logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ctx.session.update(cx, |s, cx| s.logout(cx));
        let app = self.ctx.app.clone();
        let _ = app.update(cx, |app, cx| {
            app.settings.token.clear();
            app.persist_settings(cx);
        });
        self.token_input.update(cx, |i, cx| i.set_value("", window, cx));
    }

    fn apply_accent(&mut self, cx: &mut Context<Self>) {
        let hex = self.accent_input.read(cx).value().to_string();
        let app = self.ctx.app.clone();
        let _ = app.update(cx, |app, cx| {
            app.update_settings(
                |s| {
                    s.accent = if hex.trim().is_empty() {
                        None
                    } else {
                        Some(hex)
                    };
                },
                cx,
            );
            app.apply_theme(cx);
        });
    }
}

#[derive(Clone, Copy)]
enum Flag {
    Autoplay,
    Restore,
    Covers,
}

fn section(title: &'static str, body: AnyElement) -> AnyElement {
    v_flex()
        .w_full()
        .gap_2()
        .child(div().text_lg().font_weight(FontWeight::BOLD).child(title))
        .child(body)
        .into_any_element()
}

fn chip(id: String, label: String, active: bool, on: Rc<dyn Fn(&mut Window, &mut App)>) -> AnyElement {
    Button::new(id)
        .ghost()
        .small()
        .selected(active)
        .child(label)
        .on_click(move |_, window, cx| on(window, cx))
        .into_any_element()
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.ctx.app.clone();
        let settings = self.settings.clone();
        let logged_in = self.ctx.session.read(cx).is_logged_in();
        let profile = self.ctx.session.read(cx).profile().cloned();

        // --- аккаунт ---
        let login_session = self.ctx.session.clone();
        let input_row = h_flex()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Input::new(&self.token_input)
                            .content_type(InputContentType::Password)
                            .mask_toggle(),
                    ),
            )
            .child(
                Button::new("login")
                    .primary()
                    .label(if logged_in { "Обновить токен" } else { "Войти" })
                    .on_click(cx.listener(|this, _, _, cx| this.start_login(cx))),
            )
            .child({
                let session = login_session.clone();
                if logged_in {
                    Button::new("logout")
                        .danger()
                        .label("Выйти")
                        .on_click(move |_, _window, cx| {
                            let _ = session.update(cx, |s, cx| s.logout(cx));
                        })
                        .into_any_element()
                } else {
                    div().into_any_element()
                }
            });
        let profile_row = profile.map(|p| {
            let muted = cx.theme().muted_foreground;
            h_flex()
                .gap_2()
                .items_center()
                .text_sm()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(p.display_name))
                .child(
                    div()
                        .text_color(muted)
                        .child(if p.has_plus { "Подписка Плюс" } else { "Без Плюс" }),
                )
        });
        let accent_row = h_flex()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.accent_input)),
            )
            .child(
                Button::new("apply-accent")
                    .outline()
                    .label("Применить акцент")
                    .on_click(cx.listener(|this, _, _, cx| this.apply_accent(cx))),
            );
        let account_body = v_flex()
            .gap_3()
            .child(input_row)
            .when_some(profile_row, |this, el| this.child(el))
            .child(accent_row)
            .into_any_element();

        // --- оформление ---
        let quality_chips: Vec<AnyElement> = [
            (Quality::Low, "128 кбит/с"),
            (Quality::Normal, "192 кбит/с"),
            (Quality::High, "максимум"),
            (Quality::Max, "lossless"),
        ]
        .into_iter()
        .map(|(q, label)| {
            let app = app.clone();
            chip(
                format!("quality-{}", label),
                label.to_string(),
                settings.quality == q,
                Rc::new(move |_window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.update_settings(|s| s.quality = q, cx);
                    });
                }),
            )
        })
        .collect();

        let appearance_body = v_flex()
            .gap_2()
            .child(SettingRow::new(
                "Тёмная тема",
                "Оформление интерфейса",
                Switch::new("theme-dark")
                    .checked(settings.theme_dark)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        let dark = *checked;
                        this.set_theme(dark, cx);
                    })),
            ))
            .child(SettingRow::new(
                "Обложки в боковой панели",
                "Миниатюры плейлистов слева",
                Switch::new("sidebar-covers")
                    .checked(settings.sidebar_covers)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        let v = *checked;
                        this.set_flag(Flag::Covers, v, cx);
                    })),
            ))
            .child(SettingRow::new(
                "Качество звука",
                "Что запрашивать у CDN",
                h_flex().gap_1().children(quality_chips),
            ))
            .into_any_element();

        // --- плеер ---
        let player_body = v_flex()
            .gap_2()
            .child(SettingRow::new(
                "Восстанавливать очередь",
                "После перезапуска вернутся список, позиция и режимы",
                Switch::new("restore")
                    .checked(settings.restore_session)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        let v = *checked;
                        this.set_flag(Flag::Restore, v, cx);
                    })),
            ))
            .child(SettingRow::new(
                "Автозапуск восстановленного трека",
                "Иначе очередь вернётся на паузе",
                Switch::new("autoplay")
                    .checked(settings.autoplay_restored)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        let v = *checked;
                        this.set_flag(Flag::Autoplay, v, cx);
                    })),
            ))
            .child(SettingRow::new(
                "Таймер сна",
                "Остановить воспроизведение через",
                h_flex().gap_1().children(sleep_chips(self.ctx.player.clone(), cx)),
            ))
            .into_any_element();

        // --- эквалайзер ---
        let preset_chips: Vec<AnyElement> = EQ_PRESETS
            .iter()
            .enumerate()
            .map(|(ix, (name, _))| {
                let active = settings.eq.preset == *name;
                let id = ix;
                chip(
                    format!("eq-preset-{name}"),
                    name.to_string(),
                    active,
                    {
                        let entity = cx.entity().downgrade();
                        Rc::new(move |window, cx| {
                            let _ = entity.update(cx, |this, cx| {
                                this.apply_preset(id, window, cx);
                            });
                        })
                    },
                )
            })
            .collect();

        let eq_body = v_flex()
            .gap_2()
            .child(SettingRow::new(
                "Включить эквалайзер",
                "8 полос, применяется к потоку",
                Switch::new("eq-on")
                    .checked(settings.eq.enabled)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        let v = *checked;
                        let app = this.ctx.app.clone();
                        let _ = app.update(cx, |app, cx| {
                            app.update_settings(|s| s.eq.enabled = v, cx);
                        });
                    })),
            ))
            .child(h_flex().gap_1().children(preset_chips))
            .child(
                div()
                    .w_full()
                    .gap_3()
                    .grid()
                    .grid_cols(4)
                    .children(self.eq_sliders.iter().enumerate().map(|(ix, state)| {
                        let freq = EQ_FREQUENCIES[ix];
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().secondary)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format_band(freq)),
                            )
                            .child(Slider::new(state))
                    })),
            )
            .into_any_element();

        // --- данные ---
        let data_body = v_flex()
            .gap_2()
            .child(SettingRow::new(
                "Очистить кеш",
                "Сохранённая сессия; картинный кеш живёт в памяти GPUI",
                Button::new("clear-cache")
                    .outline()
                    .label("Очистить")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let app = this.ctx.app.clone();
                        let _ = app.update(cx, |app, cx| {
                            app.store.update(cx, |store, _| store.clear_caches());
                        });
                        window.push_notification("Кеш очищен", cx);
                    })),
            ))
            .child(SettingRow::new(
                "Тексты песен",
                "Источник синхронных текстов",
                div().text_xs().child("lrclib.net"),
            ))
            .into_any_element();

        v_flex()
            .id("settings-page")
            .size_full()
            .min_w_0()
            .overflow_y_scrollbar()
            .gap_4()
            .p_6()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Настройки"),
            )
            .when_some(self.login_error.clone(), |this, err| {
                this.child(StatusBanner::error(err))
            })
            .child(section("Аккаунт", account_body))
            .child(section("Оформление", appearance_body))
            .child(section("Плеер", player_body))
            .child(section("Эквалайзер", eq_body))
            .child(section("Данные", data_body))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("meowsic 0.1 — клиент Яндекс Музыки на GPUI. Неофициальное API, нужен аккаунт с Плюс."),
            )
    }
}

fn sleep_chips(player: Entity<Player>, _cx: &mut Context<SettingsPage>) -> Vec<AnyElement> {
    [0u64, 15, 30, 45, 60, 90]
        .into_iter()
        .map(|min| {
            let label = if min == 0 {
                "Выкл".to_string()
            } else {
                format!("{min} мин")
            };
            let p = player.clone();
            chip(
                format!("sleep-{label}"),
                label,
                false,
                Rc::new(move |_window, cx| {
                    let _ = p.update(cx, |pl, cx| {
                        if min == 0 {
                            pl.cancel_sleep_timer();
                        } else {
                            pl.start_sleep_timer(min, cx);
                        }
                    });
                }),
            )
        })
        .collect()
}

fn format_band(f: f32) -> String {
    if f >= 1000.0 {
        format!("{} кГц", f / 1000.0)
    } else {
        format!("{f} Гц")
    }
}
