use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    h_flex, v_flex,
};

use gpui_kit::{Context, Entity, FontWeight, Subscription, Window, div};
use gpui_kit::prelude::*;
use crate::ui::widgets::{Cover, StatusBanner, empty_hint};
use crate::yandex::SearchDto;
use super::Load;
use super::PageCtx;
use super::Route;

pub struct SearchPage {
    ctx: PageCtx,
    query: Entity<InputState>,
    results: Load<SearchDto>,
    _subs: Vec<Subscription>,
}

impl SearchPage {
    pub fn new(ctx: PageCtx, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Артист, альбом или трек…")
                
        });
        let q = query.clone();
        let mut this = Self {
            ctx,
            query,
            results: Load::Idle,
            _subs: Vec::new(),
        };
        this._subs.push(cx.subscribe_in(&q, window, |this, _, event, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.run(cx);
            }
        }));
        this
    }

    pub fn query_input(&self) -> Entity<InputState> {
        self.query.clone()
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let text = self.query.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let api = self.ctx.session.read(cx).yandex();
        let gen_query = text.clone();
        self.results = Load::Loading;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res = api.search(&gen_query).await;
            let _ = weak.update(cx, |this, cx| {
                this.results = match res {
                    Ok(dto) => Load::Ready(dto),
                    Err(e) => Load::Failed(e.to_string().into()),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

}

impl Render for SearchPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let results = match &self.results {
            Load::Ready(dto) => Some(dto.clone()),
            _ => None,
        };
        let loading = matches!(self.results, Load::Loading);
        let error = match &self.results {
            Load::Failed(msg) => Some(msg.clone()),
            _ => None,
        };

        // данные для секций
        let (tracks, artists, albums, query_text) = match &results {
            Some(dto) => (
                dto.tracks.clone(),
                dto.artists.clone(),
                dto.albums.clone(),
                dto.text.clone(),
            ),
            None => (Vec::new(), Vec::new(), Vec::new(), String::new()),
        };

        let empty_el = if !loading && results.is_none() {
            Some(
                empty_hint(
                    "Ищи что-нибудь — тут появятся треки, артисты и альбомы",
                    cx,
                ),
            )
        } else {
            None
        };
        let track_rows: Vec<gpui_kit::AnyElement> = tracks
            .iter()
            .take(50)
            .enumerate()
            .map(|(ix, t)| {
                let row_ctx = self.row_activate(ix, tracks.clone(), query_text.clone(), cx);
                crate::ui::widgets::TrackRow::new(t.clone(), ix, row_ctx)
                    .liked(self.ctx.lib.read(cx).is_liked(&t.id))
                    .on_like({
                        let id = t.id.clone();
                        let api = self.ctx.session.read(cx).yandex();
                        let lib = self.ctx.lib.clone();
                        move |_, _, cx| {
                            lib.update(cx, |lib, cx| lib.toggle_like(&api, &id, false, cx));
                        }
                    })
                    .current(
                        self.ctx
                            .player
                            .read(cx)
                            .current
                            .as_ref()
                            .map(|c| c.id.clone())
                            == Some(t.id.clone()),
                    )
                    .into_any_element()
            })
            .collect();

        let artist_cards: Vec<gpui_kit::AnyElement> = artists
            .iter()
            .map(|a| {
                let id = a.id.clone();
                let app = self.ctx.app.clone();
                h_flex()
                    .id(format!("s-artist-{}", a.id))
                    .h_12()
                    .px_3()
                    .gap_3()
                    .items_center()
                    .cursor_pointer()
                    .rounded(theme.radius)
                    .bg(theme.secondary)
                    .hover(|s| s.bg(theme.secondary_hover))
                    .on_click(move |_, _, cx| {
                        let id = id.clone();
                        let _ = app.update(cx, |this, cx| this.navigate(Route::Artist(id), cx));
                    })
                    .child(Cover::new(a.cover_url.clone(), 40.0, format!("s-art-c-{}", a.id.clone())))
                    .child(
                        v_flex()
                            .child(div().truncate().text_sm().child(a.name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Артист"),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

        let album_cards: Vec<gpui_kit::AnyElement> = albums
            .iter()
            .map(|a| {
                let id = a.id.clone();
                let app = self.ctx.app.clone();
                div()
                    .id(format!("s-album-{}", a.id))
                    .w_40()
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        let id = id.clone();
                        let _ = app.update(cx, |this, cx| this.navigate(Route::Album(id), cx));
                    })
                    .child(Cover::new(a.cover_url.clone(), 160.0, format!("s-alb-c-{}", a.id.clone())))
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(a.title.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(
                                a.year
                                    .map(|y| y.to_string())
                                    .unwrap_or_else(|| "Альбом".to_string()),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

        v_flex()
            .id("search-page")
            .size_full()
            .min_w_0()
            .overflow_y_scrollbar()
            .gap_6()
            .p_6()
            .child(
                h_flex()
                    
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.query).prefix({
                                Icon::new(IconName::Search).small()
                            })),
                    )
                    .child(
                        Button::new("search-go")
                            .primary()
                            .label("Искать")
                            .loading(loading)
                            .on_click(cx.listener(|this, _, _, cx| this.run(cx))),
                    ),
            )
            .when_some(error, |this, err| this.child(StatusBanner::error(err)))
            .when_some(empty_el, |this, el| this.child(el))
            .when(!tracks.is_empty(), move |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .child("Треки"),
                        )
                        .children(track_rows),
                )
            })
            .when(!artists.is_empty(), move |this| {
                this.child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .child("Артисты"),
                        )
                        .child(
                            div()
                                
                                .gap_2()
                                .flex_wrap()
                                .children(artist_cards),
                        ),
                )
            })
            .when(!albums.is_empty(), move |this| {
                this.child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .child("Альбомы"),
                        )
                        .child(
                            h_flex()
                                .id("s-albums")
                                
                                .gap_3()
                                .pb_2()
                                .overflow_x_scrollbar()
                                .children(album_cards),
                        ),
                )
            })

    }
}

impl SearchPage {
    fn row_activate(
        &self,
        _ix: usize,
        tracks: Vec<crate::yandex::TrackDto>,
        query: String,
        _cx: &mut Context<Self>,
    ) -> std::rc::Rc<dyn Fn(usize, &mut Window, &mut gpui_kit::App)> {
        let player = self.ctx.player.clone();
        std::rc::Rc::new(move |ix, _window, cx| {
            let label = format!("Поиск: {query}");
            player.update(cx, |p, cx| p.play_context(&label, tracks.clone(), ix, cx));
        })
    }
}

