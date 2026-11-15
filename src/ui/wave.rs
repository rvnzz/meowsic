use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::motion::{transition, Transition};
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Load, MY_WAVE_STATION, PageCtx, parse_hex};
use crate::ui::widgets::{Cover, StatusBanner};
use crate::yandex::{StationInfoDto, TrackDto};

pub struct WavePage {
    ctx: PageCtx,
    station: String,
    info: Load<StationInfoDto>,
    viz: Entity<crate::ui::visualizer::Visualizer>,
    _subs: Vec<Subscription>,
}

/// Палитра кадра: два цвета градиента + акцент.
struct Palette {
    top: Hsla,
    bottom: Hsla,
    accent: Hsla,
}

/// Текст поверх «арт-данных»: белый на тёмном, почти-чёрный на светлом.
/// Константы — часть вычисленной палитры изображения (разрешённое исключение
/// Coding Guides: data color).
fn ink(bg: Hsla) -> Hsla {
    if bg.l < 0.62 {
        Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }.into()
    } else {
        Rgba { r: 0.05, g: 0.05, b: 0.07, a: 1.0 }.into()
    }
}

/// Смешивание двух цветов палитры (Hsla::mix в этом gpui отсутствует).
fn mix_hsla(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    // hues blend through the shorter arc of the color wheel
    let dh = {
        let d = b.h - a.h;
        if d > 0.5 {
            d - 1.0
        } else if d < -0.5 {
            d + 1.0
        } else {
            d
        }
    };
    Hsla {
        h: (a.h + dh * t + 1.0) % 1.0,
        s: lerp(a.s, b.s),
        l: lerp(a.l, b.l),
        a: lerp(a.a, b.a),
    }
}

fn slide() -> Transition {
    Transition::new(Duration::from_millis(900))
}

impl WavePage {
    pub fn new(ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        let viz = cx.new(|cx| {
            crate::ui::visualizer::Visualizer::new(
                ctx.player.clone(),
                crate::player::tap::BANDS,
                56.0,
                cx,
            )
        });
        let mut this = Self {
            ctx,
            station: MY_WAVE_STATION.to_string(),
            info: Load::Idle,
            viz,
            _subs: Vec::new(),
        };
        let player = this.ctx.player.clone();
        this._subs.push(cx.observe(&player, |this, _, cx| {
            if this.ctx.player.read(cx).wave.is_some() && matches!(this.info, Load::Idle) {
                this.reload(cx);
            }
            cx.notify();
        }));
        this
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if self.station.is_empty() {
            return;
        }
        let api = self.ctx.session.read(cx).yandex();
        let station = self.station.clone();
        let weak = cx.entity().downgrade();
        self.info = Load::Loading;
        cx.spawn(async move |_, cx| {
            let res = api.station_info(&station).await;
            let _ = weak.update(cx, |this, cx| {
                this.info = match res {
                    Ok(info) => Load::Ready(info),
                    Err(e) => Load::Failed(e.to_string().into()),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn set_setting(&mut self, language: Option<String>, mood: Option<String>, cx: &mut Context<Self>) {
        let api = self.ctx.session.read(cx).yandex();
        let station = self.station.clone();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _ = api
                .set_station_settings(&station, language.as_deref(), mood.as_deref())
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.info = Load::Idle;
                this.reload(cx);
            });
        })
        .detach();
    }

    /// Палитра: derivedColors обложки с плавным перетеканием; фолбэк — тема.
    fn palette(
        &self,
        track: Option<&TrackDto>,
        theme: &gpui_kit::component::Theme,
        window: &mut Window,
        cx: &mut App,
    ) -> Palette {
        let (mut top, mut bottom, mut accent) = (theme.secondary, theme.background, theme.primary);
        if let Some(t) = track {
            if let Some(c) = t.color_from.as_deref().and_then(parse_hex) {
                top = c;
            }
            if let Some(c) = t.color_to.as_deref().and_then(parse_hex) {
                bottom = c;
            }
            if let Some(c) = t.color_accent.as_deref().and_then(parse_hex) {
                accent = c;
            }
        }
        if top.l < 0.10 {
            top = Hsla { l: 0.13, ..top };
        }
        if bottom.l < 0.10 {
            bottom = Hsla { l: 0.15, ..bottom };
        }
        Palette {
            top: transition(ElementId::from("wave-top"), top, slide(), window, cx),
            bottom: transition(ElementId::from("wave-bottom"), bottom, slide(), window, cx),
            accent: transition(ElementId::from("wave-accent"), accent, slide(), window, cx),
        }
    }
}

impl Render for WavePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let player = self.ctx.player.read(cx);
        let in_wave = player.wave.is_some();
        let current = player.current.clone();
        let playing = player.playing;
        let loading = player.loading;
        let error = player.error.clone();
        let position = player.position_ms();
        let duration = player.duration_ms();
        drop(player);

        let entity_player = self.ctx.player.clone();
        let entity_lib = self.ctx.lib.clone();
        let api = self.ctx.session.read(cx).yandex();

        let hero_track = current.clone().filter(|_| in_wave);
        let palette = self.palette(hero_track.as_ref(), &theme, window, cx);
        let bg = linear_gradient(
            150.0,
            linear_color_stop(palette.top, 0.0),
            linear_color_stop(palette.bottom, 1.0),
        );
        let ink_main = ink(mix_hsla(palette.bottom, palette.top, 0.5));
        let ink_soft = ink_main.opacity(0.78);
        let glass = palette.top.opacity(0.42);
        let glass_border = ink_main.opacity(0.16);
        let max_w = rems(58.0);

        let circle = |id: &'static str, icon: IconName, _tip: &'static str, next: bool| {
            let p = entity_player.clone();
            div()
                .id(id)
                .size_10()
                .flex_shrink_0()
                .rounded_full()
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(glass_border)
                .hover(|s| s.bg(glass))
                .child(Icon::new(icon).small().text_color(ink_main))
                .on_click(move |_, _, cx| {
                    let _ = p.update(cx, move |pl, cx| {
                        if next {
                            pl.next(cx);
                        } else {
                            pl.prev(cx);
                        }
                    });
                })
        };

        let play_button = {
            let p = entity_player.clone();
            div()
                .id("wave-play")
                .size_16()
                .flex_shrink_0()
                .rounded_full()
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .bg(ink_main)
                .shadow_lg()
                .hover(|s| s.bg(ink_main.opacity(0.86)))
                .child(
                    Icon::new(if loading {
                        IconName::LoaderCircle
                    } else if playing {
                        IconName::Pause
                    } else {
                        IconName::Play
                    })
                    .large()
                    .text_color(ink(ink_main)),
                )
                .on_click(move |_, _, cx| {
                    let _ = p.update(cx, |pl, cx| pl.toggle_play(cx));
                })
        };

        let hero: AnyElement = if let Some(track) = hero_track.clone() {
            let id = track.id.clone();
            let liked = self.ctx.lib.read(cx).is_liked(&id);
            let disliked = self.ctx.lib.read(cx).is_disliked(&id);
            let pct = if duration > 0 {
                (position.max(0) as f32 / duration.max(1) as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let elapsed_s = (position.max(0) / 1000) as i64;
            let remain_s = ((duration - position).max(0) / 1000) as i64;

            h_flex()
                .w_full()
                .gap_8()
                .items_center()
                // левая колонка: светящаяся обложка + прогресс под ней
                .child(
                    v_flex()
                        .gap_4()
                        .flex_shrink_0()
                        .child(
                            div()
                                .size_72()
                                .overflow_hidden()
                                .rounded_2xl()
                                .shadow_2xl()
                                .border_1()
                                .border_color(glass_border)
                                .child(Cover::new(
                                    track.cover_url_large.clone(),
                                    288.0,
                                    format!("wave-hero-{id}"),
                                )),
                        )
                        .child(
                            v_flex()
                                .w_72()
                                .gap_1()
                                .child(
                                    div()
                                        .w_full()
                                        .h_1p5()
                                        .overflow_hidden()
                                        .rounded_full()
                                        .bg(ink_main.opacity(0.14))
                                        .child(
                                            div()
                                                .h_full()
                                                .w(rems(18.0 * pct))
                                                .rounded_full()
                                                .bg(ink_main),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .w_full()
                                        .justify_between()
                                        .text_xs()
                                        .text_color(ink_soft)
                                        .child(format!(
                                            "{:02}:{:02}",
                                            elapsed_s / 60,
                                            elapsed_s % 60
                                        ))
                                        .child(format!(
                                            "осталось {}:{:02}",
                                            remain_s / 60,
                                            remain_s % 60
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .w_72()
                                .h_16()
                                .child(self.viz.clone()),
                        ),
                )
                // правая колонка: текст и управление
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(ink_soft)
                                .child("МОЯ ВОЛНА · НЕПРЕРЫВНЫЙ ПОТОК"),
                        )
                        .child(
                            div()
                                .text_3xl()
                                .font_weight(FontWeight::EXTRA_BOLD)
                                .text_color(ink_main)
                                                                .child(track.title.clone()),
                        )
                        .child(
                            div()
                                .text_lg()
                                .text_color(ink_soft)
                                .child(track.artist_line()),
                        )
                        .when_some(track.album_title.clone(), move |this, album| {
                            this.child(div().text_sm().text_color(ink_soft).child(album))
                        })
                        .child(div().min_h_4())
                        .child(
                            h_flex()
                                .gap_4()
                                .items_center()
                                .child(circle("wave-prev", IconName::SkipBack, "Назад", false))
                                .child(play_button)
                                .child(circle("wave-next", IconName::SkipForward, "Пропустить", true)),
                        )
                        .child(
                            h_flex()
                                .gap_3()
                                .mt_4()
                                .child({
                                    let api = api.clone();
                                    let lib = entity_lib.clone();
                                    let p = entity_player.clone();
                                    let id = id.clone();
                                    Button::new(format!("wave-like-{id}"))
                                        .ghost()
                                        .bg(glass)
                                        .text_color(ink_main)
                                        .icon(if liked {
                                            IconName::Heart
                                        } else {
                                            IconName::HeartOff
                                        })
                                        .label(if liked { "Лайк стоит" } else { "Лайк" })
                                        .on_click(move |_, _, cx| {
                                            let id = id.clone();
                                            let _ = lib.update(cx, |lib, cx| {
                                                lib.toggle_like(&api, &id, false, cx);
                                            });
                                            let _ = p.update(cx, |pl, cx| {
                                                pl.wave_feedback_pub("like", cx);
                                            });
                                        })
                                })
                                .child({
                                    let api = api.clone();
                                    let lib = entity_lib.clone();
                                    let p = entity_player.clone();
                                    let id = id.clone();
                                    Button::new(format!("wave-dislike-{id}"))
                                        .ghost()
                                        .bg(glass)
                                        .text_color(if disliked { theme.danger } else { ink_main })
                                        .icon(IconName::ThumbsDown)
                                        .label("Дизлайк")
                                        .on_click(move |_, _, cx| {
                                            let id = id.clone();
                                            let _ = lib.update(cx, |lib, cx| {
                                                lib.toggle_like(&api, &id, true, cx);
                                            });
                                            let _ = p.update(cx, |pl, cx| {
                                                pl.wave_feedback_pub("dislike", cx);
                                                pl.next(cx);
                                            });
                                        })
                                }),
                        ),
                )
                .into_any_element()
        } else if in_wave {
            v_flex()
                .w_full()
                .items_center()
                .py_24()
                .child(
                    div()
                        .text_lg()
                        .text_color(ink_soft)
                        .child(if loading { "Загружаю первый трек…" } else { "Волна настраивается…" }),
                )
                .into_any_element()
        } else {
            v_flex()
                .w_full()
                .justify_center()
                .py_10()
                .child(
                    div()
                        .mx_auto()
                        .w_full()
                        .max_w(rems(28.0))
                        .child(
                            v_flex()
                                .w_full()
                                .items_center()
                                .gap_4()
                                .px_8()
                                .py_10()
                                .rounded_2xl()
                                .bg(glass)
                                .border_1()
                                .border_color(glass_border)
                                .child(
                                    div()
                                        .size_12()
                                        .rounded_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(palette.accent.opacity(0.16))
                                        .border_1()
                                        .border_color(palette.accent.opacity(0.5))
                                        .child(
                                            Icon::new(IconName::Radio)
                                                .small()
                                                .text_color(palette.accent),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(ink_main)
                                        .child("Волна остановлена"),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_center()
                                        .text_color(ink_soft)
                                        .child(
                                            "Персональный поток, который подстраивается \nпод твои лайки и дизлайки с каждой песней",
                                        ),
                                )
                                .child(
                                    Button::new("wave-start")
                                        .primary()
                                        .large()
                                        .icon(IconName::Play)
                                        .label("Запустить мою волну")
                                        .on_click(cx.listener({
                                            let p = entity_player.clone();
                                            move |_, _, _, cx| {
                                                let _ = p.update(cx, |pl, cx| {
                                                    pl.start_wave(MY_WAVE_STATION, "Моя волна", cx)
                                                });
                                            }
                                        })),
                                ),
                        ),
                )
                .into_any_element()
                };

        let settings_card: AnyElement = match &self.info {
            Load::Idle | Load::Loading => {
                if in_wave {
                    tinted_note(if matches!(self.info, Load::Loading) { "Читаю настройки станции…" } else { "Настройке волны ещё учатся…" }, ink_soft)
                } else {
                    div().into_any_element()
                }
            }
            Load::Failed(msg) => StatusBanner::error(msg.clone()).into_any_element(),
            Load::Ready(info) => {
                let info = info.clone();
                let lang = info.language.clone();
                let mood = info.mood_energy.clone();
                let ent = cx.entity().downgrade();
                let glass_chip = glass;
                let border_chip = glass_border;
                let ink_chip = ink_main;
                let ink_btn = ink(ink_main);

                let mk_chip = move |group: &'static str,
                                    value: String,
                                    name: String,
                                    active: bool,
                                    lang_i: Option<String>,
                                    mood_i: Option<String>| {
                    let ent = ent.clone();
                    div()
                        .id(format!("{group}-{value}"))
                        .px_3()
                        .py_1()
                        .rounded_full()
                        .cursor_pointer()
                        .text_sm()
                        .border_1()
                        .border_color(if active { glass_chip } else { border_chip })
                        .bg(if active { ink_chip } else { glass_chip })
                        .text_color(if active { ink_btn } else { ink_chip })
                        .hover(|s| s.bg(if active { ink_chip } else { border_chip }))
                        .child(name)
                        .on_click(move |_, _, cx| {
                            let value = value.clone();
                            let lang_i = lang_i.clone();
                            let mood_i = mood_i.clone();
                            let _ = ent.update(cx, |this, cx| match group {
                                "lang" => this.set_setting(
                                    Some(value),
                                    mood_i.filter(|s| !s.is_empty()),
                                    cx,
                                ),
                                _ => this.set_setting(
                                    lang_i.filter(|s| !s.is_empty()),
                                    Some(value),
                                    cx,
                                ),
                            });
                        })
                };

                let lang_chips: Vec<_> = std::iter::once((String::new(), "Не важно".to_string()))
                    .chain(
                        info.languages
                            .iter()
                            .map(|o| (o.value.clone(), o.name.clone())),
                    )
                    .map(|(value, name)| {
                        let active = lang.as_deref().unwrap_or_default() == value.as_str();
                        mk_chip("lang", value, name, active, lang.clone(), mood.clone())
                    })
                    .collect();
                let mood_chips: Vec<_> = std::iter::once((String::new(), "Как всегда".to_string()))
                    .chain(info.moods.iter().map(|o| (o.value.clone(), o.name.clone())))
                    .map(|(value, name)| {
                        let active = mood.as_deref().unwrap_or_default() == value.as_str();
                        mk_chip("mood", value, name, active, lang.clone(), mood.clone())
                    })
                    .collect();

                v_flex()
                    .w_full()
                    .gap_4()
                    .p_5()
                    .rounded_2xl()
                    .bg(glass)
                    .border_1()
                    .border_color(glass_border)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(ink_main)
                            .child("НАСТРОЙКИ ВОЛНЫ"),
                    )
                    .child(h_flex().gap_2().flex_wrap().children(lang_chips))
                    .child(h_flex().gap_2().flex_wrap().children(mood_chips))
                    .into_any_element()
            }
        };

        v_flex()
            .id("wave-page")
            .size_full()
            .min_w_0()
            .bg(bg)
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .w_full()
                    .max_w(max_w)
                    .mx_auto()
                    .gap_10()
                    .px_8()
                    .pt_8()
                    .pb_12()
                    .child(
                        h_flex()
                            .w_full()
                            .gap_3()
                            .items_center()
                            .child(
                                div()
                                    .size_8()
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(palette.accent)
                                    .child(
                                        Icon::new(IconName::Radio)
                                            .xsmall()
                                            .text_color(ink(palette.accent)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::EXTRA_BOLD)
                                    .text_color(ink_main)
                                    .child("Моя волна"),
                            ),
                    )
                    .when_some(error, |this, err| this.child(StatusBanner::error(err)))
                    .child(hero)
                    .when_some(if in_wave { Some(settings_card) } else { None }, |this, el| {
                        this.child(el)
                    }),
            )
    }
}

fn tinted_note(msg: &'static str, color: Hsla) -> AnyElement {
    div()
        .w_full()
        .py_10()
        .flex()
        .flex_col()
        .items_center()
        .text_color(color)
        .child(msg)
        .into_any_element()
}
