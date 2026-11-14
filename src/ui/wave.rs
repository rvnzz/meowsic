use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Sizable, Selectable,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    h_flex, v_flex,
};

use gpui_kit::{Context, FontWeight, Subscription, Window, div};
use gpui_kit::prelude::*;
use crate::ui::widgets::{Cover, StatusBanner, empty_hint};
use crate::yandex::StationInfoDto;
use super::Load;
use super::MY_WAVE_STATION;
use super::PageCtx;

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
            if this.ctx.player.read(cx).wave.is_some()
                && matches!(this.info, Load::Idle)
            {
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
            let _ = weak.update(cx, |this, cx| this.reload(cx));
        })
        .detach();
    }

}

impl Render for WavePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let player = self.ctx.player.read(cx);
        let in_wave = player.wave.is_some();
        let current = player.current.clone();
        let playing = player.playing;
        let loading = player.loading;
        let error = player.error.clone();
        drop(player);
        let entity_player = self.ctx.player.clone();
        let entity_lib = self.ctx.lib.clone();
        let api = self.ctx.session.read(cx).yandex();

        let body = if let Some(track) = current.filter(|_| in_wave) {
            let id = track.id.clone();
            let liked = self.ctx.lib.read(cx).is_liked(&id);
            let disliked = self.ctx.lib.read(cx).is_disliked(&id);
            v_flex()
                
                .items_center()
                .gap_6()
                .child(Cover::new(track.cover_url_large.clone(), 260.0, format!("wave-cover-{}", id.clone())))
                .child(
                    v_flex()
                        .items_center()
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
                        .when_some(track.album_title.clone(), |this, album| {
                            this.child(
                                div().text_sm().text_color(theme.muted_foreground).child(album),
                            )
                        }),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
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
                                .on_click(cx.listener({
                                    let p = entity_player.clone();
                                    move |_, _, _, cx| {
                                        p.update(cx, |pl, cx| pl.toggle_play(cx));
                                    }
                                })),
                        )
                        .child(
                            Button::new("wave-prev")
                                .ghost()
                                .large()
                                .icon(IconName::SkipBack)
                                .on_click(cx.listener({
                                    let p = entity_player.clone();
                                    move |_, _, _, cx| p.update(cx, |pl, cx| pl.prev(cx))
                                })),
                        )
                        .child(
                            Button::new("wave-skip")
                                .ghost()
                                .large()
                                .icon(IconName::SkipForward)
                                .tooltip("Пропустить (в волну уйдёт skipForward)")
                                .on_click(cx.listener({
                                    let p = entity_player.clone();
                                    move |_, _, _, cx| p.update(cx, |pl, cx| pl.next(cx))
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child({
                            let id = id.clone();
                            let api = api.clone();
                            let lib = entity_lib.clone();
                            let p = entity_player.clone();
                            Button::new(format!("wave-like-{}", id))
                                .outline()
                                .icon(if liked { IconName::Heart } else { IconName::HeartOff })
                                .label(if liked { "Лайк стоит" } else { "Лайк" })
                                .text_color(if liked { theme.primary } else { theme.foreground })
                                .on_click(move |_, _, cx| {
                                    let id = id.clone();
                                    lib.update(cx, |lib, cx| {
                                        lib.toggle_like(&api, &id, false, cx);
                                    });
                                    p.update(cx, |pl, cx| {
                                        pl.wave_feedback_pub("like", cx);
                                    });
                                })
                        })
                        .child({
                            let id = id.clone();
                            let api = api.clone();
                            let lib = entity_lib.clone();
                            let p = entity_player.clone();
                            Button::new(format!("wave-dislike-{}", id))
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
                                    lib.update(cx, |lib, cx| {
                                        lib.toggle_like(&api, &id, true, cx);
                                    });
                                    p.update(cx, |pl, cx| {
                                        pl.wave_feedback_pub("dislike", cx);
                                        pl.next(cx);
                                    });
                                })
                        }),
                )
        } else if in_wave || loading {
            v_flex()
                
                .items_center()
                .py_12()
                .child(
                    div()
                        .text_lg()
                        .text_color(theme.muted_foreground)
                        .child("Волна настраивается…"),
                )
        } else {
            v_flex()
                
                .items_center()
                .gap_4()
                .py_12()
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
                                p.update(cx, |pl, cx| {
                                    pl.start_wave(MY_WAVE_STATION, "Моя волна", cx)
                                });
                            }
                        })),
                )
        };

        let settings_block = match &self.info {
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
                .chain(info.languages.iter().map(|o| (o.value.clone(), o.name.clone())))
                .collect();
                let mood_opts: Vec<(String, String)> = std::iter::once((
                    String::new(),
                    "Как всегда".to_string(),
                ))
                .chain(info.moods.iter().map(|o| (o.value.clone(), o.name.clone())))
                .collect();
                v_flex()
                    
                    .gap_3()
                    .p_4()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().secondary)
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
                            .overflow_x_hidden()
                            .children(lang_opts.into_iter().map(|(value, name)| {
                                let active = lang.as_deref().unwrap_or_default() == value.as_str();
                                let p = entity_player.clone();
                                let info_lang = value.clone();
                                let mood_keep = mood.clone();
                                Button::new(format!("lang-{}", value))
                                    .ghost()
                                    .small()
                                    .selected(active)
                                    .bg(if active {
                                        theme.accent
                                    } else {
                                        theme.secondary
                                    })
                                    .child(name)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_setting(
                                            Some(info_lang.clone()),
                                            mood_keep.clone().filter(|s| !s.is_empty()),
                                            cx,
                                        );
                                        let _ = p;
                                    }))
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .children(mood_opts.into_iter().map(|(value, name)| {
                                let active = mood.as_deref().unwrap_or_default() == value.as_str();
                                let lang_keep = lang.clone();
                                let info_mood = value.clone();
                                Button::new(format!("mood-{}", value))
                                    .ghost()
                                    .small()
                                    .selected(active)
                                    .bg(if active {
                                        theme.accent
                                    } else {
                                        theme.secondary
                                    })
                                    .child(name)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_setting(
                                            lang_keep.clone().filter(|s| !s.is_empty()),
                                            Some(info_mood.clone()),
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
            .gap_6()
            .p_6()
            .items_center()
            .child(
                h_flex()
                    
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Моя волна"),
                    ),
            )
            .when_some(error, |this, err| {
                this.child(StatusBanner::error(err))
            })
            .child(body)
            .child(settings_block)
    }
}
