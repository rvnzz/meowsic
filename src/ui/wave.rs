use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Selectable, Sizable, Size,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Load, MY_WAVE_STATION, PageCtx};
use crate::ui::widgets::{Cover, StatusBanner, empty_hint};
use crate::yandex::StationInfoDto;

pub struct WavePage {
    ctx: PageCtx,
    station: String,
    info: Load<StationInfoDto>,
    _subs: Vec<Subscription>,
}

impl WavePage {
    pub fn new(ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            ctx,
            station: MY_WAVE_STATION.to_string(),
            info: Load::Idle,
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
}

impl Render for WavePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let player = self.ctx.player.read(cx);
        let in_wave = player.wave.is_some();
        let current = player.current.clone().filter(|_| in_wave);
        let playing = player.playing;
        let loading = player.loading;
        let error = player.error.clone();
        drop(player);

        let fg_muted = theme.muted_foreground;
        let entity_player = self.ctx.player.clone();
        let entity_lib = self.ctx.lib.clone();
        let api = self.ctx.session.read(cx).yandex();

        let body = if let Some(track) = current {
            let id = track.id.clone();
            let liked = self.ctx.lib.read(cx).is_liked(&id);
            let disliked = self.ctx.lib.read(cx).is_disliked(&id);
            h_flex()
                .w_full()
                .gap_5()
                .items_start()
                .p_5()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .child(Cover::new(
                    track.cover_url_large.clone(),
                    240.0,
                    format!("wave-cover-{id}"),
                ))
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .text_2xl()
                                .font_weight(FontWeight::BOLD)
                                .child(track.title.clone()),
                        )
                        .child(
                            div()
                                .text_base()
                                .text_color(theme.muted_foreground)
                                .child(track.artist_line()),
                        )
                        .when_some(track.album_title.clone(), move |this, album| {
                            this.child(div().text_sm().text_color(fg_muted).child(album))
                        })
                        .child(div().flex_1().min_h_4())
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    Button::new("wave-prev")
                                        .ghost()
                                        .large()
                                        .icon(IconName::SkipBack)
                                        .tooltip("Назад")
                                        .on_click(cx.listener({
                                            let p = entity_player.clone();
                                            move |_, _, _, cx| {
                                                let _ = p.update(cx, |pl, cx| pl.prev(cx));
                                            }
                                        })),
                                )
                                .child(
                                    Button::new("wave-play")
                                        .primary()
                                        .large()
                                        .loading(loading)
                                        .icon(if playing {
                                            IconName::Pause
                                        } else {
                                            IconName::Play
                                        })
                                        .tooltip(if playing { "Пауза" } else { "Играть" })
                                        .on_click(cx.listener({
                                            let p = entity_player.clone();
                                            move |_, _, _, cx| {
                                                let _ = p.update(cx, |pl, cx| pl.toggle_play(cx));
                                            }
                                        })),
                                )
                                .child(
                                    Button::new("wave-skip")
                                        .ghost()
                                        .large()
                                        .icon(IconName::SkipForward)
                                        .tooltip("Пропустить")
                                        .on_click(cx.listener({
                                            let p = entity_player.clone();
                                            move |_, _, _, cx| {
                                                let _ = p.update(cx, |pl, cx| pl.next(cx));
                                            }
                                        })),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .mt_2()
                                .child({
                                    let api = api.clone();
                                    let lib = entity_lib.clone();
                                    let p = entity_player.clone();
                                    let id = id.clone();
                                    Button::new(format!("wave-like-{id}"))
                                        .outline()
                                        .icon(if liked {
                                            IconName::Heart
                                        } else {
                                            IconName::HeartOff
                                        })
                                        .label(if liked { "Лайк стоит" } else { "Лайк" })
                                        .text_color(if liked {
                                            theme.primary
                                        } else {
                                            theme.foreground
                                        })
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
                                        .outline()
                                        .icon(IconName::ThumbsDown)
                                        .label("Дизлайк")
                                        .text_color(if disliked {
                                            theme.danger
                                        } else {
                                            theme.foreground
                                        })
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
        } else if in_wave {
            v_flex()
                .w_full()
                .items_center()
                .gap_2()
                .py_16()
                .child(
                    div()
                        .text_lg()
                        .text_color(theme.muted_foreground)
                        .child(if loading {
                            "Загружаю первый трек…"
                        } else {
                            "Волна настраивается…"
                        }),
                )
        } else {
            v_flex()
                .w_full()
                .items_center()
                .gap_4()
                .py_16()
                .child(empty_hint("Волна остановлена", cx))
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
                )
        };

        let settings_block: AnyElement = match &self.info {
            Load::Idle => div().into_any_element(),
            Load::Loading => empty_hint("Читаю настройки станции…", cx),
            Load::Failed(msg) => StatusBanner::error(msg.clone()).into_any_element(),
            Load::Ready(info) => {
                let info = info.clone();
                let lang = info.language.clone();
                let mood = info.mood_energy.clone();
                let lang_opts: Vec<(String, String)> = std::iter::once((
                    String::new(),
                    "Не важно".to_string(),
                ))
                .chain(
                    info.languages
                        .iter()
                        .map(|o| (o.value.clone(), o.name.clone())),
                )
                .collect();
                let mood_opts: Vec<(String, String)> = std::iter::once((
                    String::new(),
                    "Как всегда".to_string(),
                ))
                .chain(info.moods.iter().map(|o| (o.value.clone(), o.name.clone())))
                .collect();
                v_flex()
                    .w_full()
                    .gap_3()
                    .p_4()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Настройки волны"),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .children(lang_opts.into_iter().map(|(value, name)| {
                                let active =
                                    lang.as_deref().unwrap_or_default() == value.as_str();
                                let mood_i = mood.clone();
                                Button::new(format!("lang-{value}"))
                                    .ghost()
                                    .small()
                                    .selected(active)
                                    .text_color(if active {
                                        theme.primary
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child(name)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_setting(
                                            Some(value.clone()),
                                            mood_i.clone().filter(|s| !s.is_empty()),
                                            cx,
                                        );
                                    }))
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .children(mood_opts.into_iter().map(|(value, name)| {
                                let active =
                                    mood.as_deref().unwrap_or_default() == value.as_str();
                                let lang_i = lang.clone();
                                Button::new(format!("mood-{value}"))
                                    .ghost()
                                    .small()
                                    .selected(active)
                                    .text_color(if active {
                                        theme.primary
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child(name)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_setting(
                                            lang_i.clone().filter(|s| !s.is_empty()),
                                            Some(value.clone()),
                                            cx,
                                        );
                                    }))
                            })),
                    )
                    .into_any_element()
            }
        };

        v_flex()
            .id("wave-page")
            .size_full()
            .min_w_0()
            .overflow_y_scrollbar()
            .gap_4()
            .p_6()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Моя волна"),
            )
            .when_some(error, |this, err| this.child(StatusBanner::error(err)))
            .child(
                div()
                    .w_full()
                    .max_w(rems(54.0))
                    .mx_auto()
                    .flex_1()
                    .child(body),
            )
            .child(
                div()
                    .w_full()
                    .max_w(rems(54.0))
                    .mx_auto()
                    .pb_6()
                    .child(settings_block),
            )
    }
}

