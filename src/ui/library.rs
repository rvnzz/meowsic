use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    button::{Button, ButtonVariants},
    dialog::{DialogClose, DialogFooter},
    input::{Input, InputState},
    scroll::ScrollableElement,
    h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{PageCtx, Route};
use crate::lib_index::LibEvent;
use crate::ui::widgets::{Cover, StatusBanner, empty_hint, fmt_count};

pub struct LibraryPage {
    ctx: PageCtx,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl LibraryPage {
    pub fn new(ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            ctx,
            error: None,
            _subs: Vec::new(),
        };
        let lib = this.ctx.lib.clone();
        this._subs.push(cx.subscribe(&lib, |this, _, event, cx| {
            match event {
                LibEvent::Changed => cx.notify(),
                LibEvent::Error(msg) => {
                    this.error = Some(msg.clone());
                    cx.notify();
                }
            }
        }));
        this
    }

    pub fn reload_if_ready(&mut self, cx: &mut Context<Self>) {
        let api = self.ctx.session.read(cx).yandex();
        self.ctx.lib.update(cx, |lib, cx| lib.refresh(&api, cx));
    }

    fn uid(&self, cx: &mut Context<Self>) -> i64 {
        self.ctx
            .session
            .read(cx)
            .profile()
            .map(|p| p.uid)
            .unwrap_or(0)
    }

    fn play_playlist(&mut self, kind: i64, title: String, cx: &mut Context<Self>) {
        let api = self.ctx.session.read(cx).yandex();
        let uid = self.uid(cx);
        let player = self.ctx.player.clone();
        cx.spawn(async move |_, cx| {
            if let Ok(pl) = api.playlist(uid, kind).await {
                let _ = player.update(cx, move |p, cx| {
                    p.play_context(&title, pl.tracks, 0, cx);
                });
            }
        })
        .detach();
    }

    fn open_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Название плейлиста"));
        let api = self.ctx.session.read(cx).yandex();
        let uid = self.uid(cx);
        let lib = self.ctx.lib.clone();
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let state = title.clone();
            let api = api.clone();
            let lib = lib.clone();
            dialog
                .title("Новый плейлист")
                .child(Input::new(&title))
                .footer(
                    DialogFooter::new()
                        .gap_2()
                        .child(DialogClose::new().trigger(|b| b.label("Отмена").outline()))
                        .child(
                            Button::new("pl-create")
                                .primary()
                                .label("Создать")
                                .on_click(move |_, window, cx| {
                                    let api = api.clone();
                                    let value = state.read(cx).value().to_string();
                                    let lib = lib.clone();
                                    cx.spawn(async move |cx| {
                                        if let Ok(created) = api.playlist_create(uid, &value, false).await {
                                            let _ = lib.update(cx, |lib, cx| {
                                                let mut list = lib.playlists.clone();
                                                list.insert(0, created);
                                                lib.replace_playlists(list, cx);
                                            });
                                        }
                                    })
                                    .detach();
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    fn open_rename_dialog(&mut self, kind: i64, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.ctx.session.read(cx).yandex();
        let uid = self.uid(cx);
        let lib = self.ctx.lib.clone();
        let current = self
            .ctx
            .lib
            .read(cx)
            .playlists
            .iter()
            .find(|p| p.kind == kind)
            .map(|p| p.title.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Новое название")
                .default_value(current)
        });
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let state = input.clone();
            let api = api.clone();
            let lib = lib.clone();
            dialog
                .title("Переименовать плейлист")
                .child(Input::new(&input))
                .footer(
                    DialogFooter::new().gap_2().child(
                        DialogClose::new().trigger(|b| b.label("Отмена").outline()),
                    ).child(
                        Button::new("pl-rename-ok")
                            .primary()
                            .label("Сохранить")
                            .on_click(move |_, window, cx| {
                                let api = api.clone();
                                let value = state.read(cx).value().to_string();
                                let lib = lib.clone();
                                cx.spawn(async move |cx| {
                                    if api.playlist_rename(uid, kind, &value).await.is_ok() {
                                        let _ = lib.update(cx, |lib, cx| {
                                            let mut list = lib.playlists.clone();
                                            if let Some(p) = list.iter_mut().find(|p| p.kind == kind) {
                                                p.title = value.clone();
                                            }
                                            lib.replace_playlists(list, cx);
                                        });
                                    }
                                })
                                .detach();
                                window.close_dialog(cx);
                            }),
                    ),
                )
        });
    }

    fn confirm_delete(&mut self, kind: i64, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.ctx.session.read(cx).yandex();
        let uid = self.uid(cx);
        let lib = self.ctx.lib.clone();
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let api = api.clone();
            let lib = lib.clone();
            alert
                .title("Удалить плейлист?")
                .description("Отменить удаление нельзя.")
                .confirm()
                .ok_text("Удалить")
                .on_ok(move |_, _window, cx| {
                    let api = api.clone();
                    let lib = lib.clone();
                    cx.spawn(async move |cx| {
                        if api.playlist_remove(uid, kind).await.is_ok() {
                            let _ = lib.update(cx, |lib, cx| {
                                let list: Vec<_> = lib
                                    .playlists
                                    .iter()
                                    .filter(|p| p.kind != kind)
                                    .cloned()
                                    .collect();
                                lib.replace_playlists(list, cx);
                            });
                        }
                    })
                    .detach();
                    true
                })
        });
    }
}

impl Render for LibraryPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let logged = self.ctx.session.read(cx).is_logged_in();
        let playlists = self.ctx.lib.read(cx).playlists.clone();

        let empty = if playlists.is_empty() {
            Some(empty_hint("Плейлистов пока нет", cx))
        } else {
            None
        };
        let error = self.error.clone().map(|e| StatusBanner::error(e));
        let gate = if !logged {
            Some(StatusBanner::error("Войди по токену в настройках, чтобы увидеть библиотеку"))
        } else {
            None
        };

        v_flex()
            .id("library-page")
            .size_full()
            .min_w_0()
            .overflow_y_scrollbar()
            .gap_5()
            .p_6()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Библиотека"),
            )
            .when_some(gate, |this, el| this.child(el))
            .when_some(error, |this, el| this.child(el))
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        Button::new("lib-liked")
                            .outline()
                            .icon(IconName::Heart)
                            .label("Мне нравится")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.ctx.app.update(cx, |app, cx| {
                                    app.navigate(Route::Liked, cx)
                                });
                            })),
                    )
                    .child(
                        Button::new("lib-refresh")
                            .ghost()
                            .icon(IconName::RotateCcw)
                            .label("Обновить")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reload_if_ready(cx);
                            })),
                    )
                    .child(
                        Button::new("lib-create")
                            .primary()
                            .icon(IconName::Plus)
                            .label("Новый плейлист")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_create_dialog(window, cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .child("Плейлисты"),
                    )
                    .when_some(empty, |this, el| this.child(el))
                    .children(playlists.iter().map(|p| {
                        let kind = p.kind;
                        let title = p.title.clone();
                        let count = p.track_count;
                        let cover = p.cover_url.clone();
                        let r = Route::Playlist(kind);

                        h_flex()
                            .id(format!("lib-pl-{kind}"))
                            .w_full()
                            .h_16()
                            .px_3()
                            .gap_3()
                            .items_center()
                            .rounded(theme.radius)
                            .bg(theme.secondary)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.secondary_hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let _ = this
                                    .ctx
                                    .app
                                    .update(cx, |app, cx| app.navigate(r.clone(), cx));
                            }))
                            .child(Cover::new(cover, 48.0, format!("libpl-c-{kind}")))
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(title.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(format!("{} треков", fmt_count(count))),
                                    ),
                            )
                            .child({
                                let title = title.clone();
                                Button::new(format!("pl-play-{kind}"))
                                    .ghost()
                                    .icon(IconName::Play)
                                    .tooltip("Слушать")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.play_playlist(kind, title.clone(), cx);
                                    }))
                            })
                            .child(
                                Button::new(format!("pl-rename-{kind}"))
                                    .ghost()
                                    .icon(IconName::Pencil)
                                    .tooltip("Переименовать")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_rename_dialog(kind, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(format!("pl-delete-{kind}"))
                                    .ghost()
                                    .danger()
                                    .icon(IconName::Trash)
                                    .tooltip("Удалить")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_delete(kind, window, cx);
                                    })),
                            )
                    })),
            )
    }
}
