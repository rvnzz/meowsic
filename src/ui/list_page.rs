use gpui_kit::assets::IconName;
use std::rc::Rc;
use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    v_virtual_list, VirtualListScrollHandle,
    h_flex, v_flex,
};

use gpui_kit::{AnyElement, Context, FontWeight, Pixels, Subscription, Window, div, px, size};
use gpui_kit::prelude::*;
use crate::ui::widgets::{Cover, StatusBanner, TrackRow, empty_hint, fmt_count};
use crate::yandex::{AlbumBriefDto,TrackDto};
use super::Load;
use super::PageCtx;
use super::Route;

#[derive(Clone, PartialEq, Eq)]
pub enum ListKind {
    Playlist(i64),
    Album(String),
    Artist(String),
    Liked,
}

#[derive(Clone)]
pub struct ListData {
    title: String,
    subtitle: String,
    cover: Option<String>,
    tracks: Vec<TrackDto>,
    albums: Vec<AlbumBriefDto>,
}

pub struct ListPage {
    ctx: PageCtx,
    kind: ListKind,
    data: Load<ListData>,
    loading_tracks: bool,
    tracks_rc: Rc<Vec<TrackDto>>,
    context_label: String,
    can_remove: bool,
    row_height: f32,
    sizes: Rc<Vec<gpui_kit::Size<gpui_kit::Pixels>>>,
    sizes_len: usize,
    sizes_rem: Pixels,
    scroll: VirtualListScrollHandle,
    _subs: Vec<Subscription>,
}

impl ListPage {
    pub fn liked(ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        Self::new(ListKind::Liked, ctx, cx)
    }

    pub fn playlist(kind: i64, ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        Self::new(ListKind::Playlist(kind), ctx, cx)
    }

    pub fn album(id: String, ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        Self::new(ListKind::Album(id), ctx, cx)
    }

    pub fn artist(id: String, ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        Self::new(ListKind::Artist(id), ctx, cx)
    }

    fn new(kind: ListKind, ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            ctx,
            kind,
            data: Load::Loading,
            loading_tracks: false,
            tracks_rc: Rc::new(Vec::new()),
            context_label: String::new(),
            can_remove: false,
            row_height: 48.0,
            sizes: Rc::new(Vec::new()),
            sizes_len: 0,
            sizes_rem: px(0.),
            scroll: VirtualListScrollHandle::new(),
            _subs: Vec::new(),
        };
        let session = this.ctx.session.clone();
        this._subs.push(cx.subscribe(&session, |this, _, event, cx| {
            if matches!(event, crate::session::SessionEvent::LoggedIn)
                && matches!(this.data, Load::Failed(_))
            {
                this.load(cx);
            }
        }));
        this.load(cx);
        this
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let api = self.ctx.session.read(cx).yandex();
        let uid = self
            .ctx
            .session
            .read(cx)
            .profile()
            .map(|p| p.uid)
            .unwrap_or(0);
        if uid == 0 && api.has_token() {
            // профиль ещё не доехал — перезагрузимся на LoggedIn
            self.data = Load::Loading;
            cx.notify();
            return;
        }
        self.data = Load::Loading;
        let kind = self.kind.clone();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res: Result<ListData, crate::yandex::ApiError> = match kind {
                ListKind::Playlist(k) => api.playlist(uid, k).await.map(|pl| ListData {
                    title: pl.title,
                    subtitle: format!("{} треков", fmt_count(pl.tracks.len() as i64)),
                    cover: None,
                    tracks: pl.tracks,
                    albums: Vec::new(),
                }),
                ListKind::Album(id) => api.album(&id).await.map(|al| ListData {
                    title: al.title.clone(),
                    subtitle: format!(
                        "{} • {}",
                        al.year.map(|y| y.to_string()).unwrap_or_default(),
                        al.artists
                            .iter()
                            .map(|a| a.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    cover: al.cover_url,
                    tracks: al.tracks,
                    albums: Vec::new(),
                }),
                ListKind::Artist(id) => api.artist(&id).await.map(|ar| ListData {
                    title: ar.name.clone(),
                    subtitle: ar
                        .listeners
                        .map(|l| format!("{} слушателей в месяц", fmt_count(l)))
                        .unwrap_or_default(),
                    cover: ar.cover_url,
                    tracks: ar.tracks,
                    albums: ar.albums,
                }),
                ListKind::Liked => api.liked_tracks(uid).await.map(|tr| ListData {
                    title: "Мне нравится".to_string(),
                    subtitle: format!("{} треков", fmt_count(tr.len() as i64)),
                    cover: None,
                    tracks: tr,
                    albums: Vec::new(),
                }),
            };
            let _ = weak.update(cx, |this, cx| {
                this.data = match res {
                    Ok(data) => Load::Ready(data),
                    Err(e) => Load::Failed(e.to_string().into()),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn play_at(&self, ix: usize, cx: &mut gpui_kit::App) {
        let (label, tracks) = match &self.data {
            Load::Ready(data) => (data.title.clone(), data.tracks.clone()),
            _ => return,
        };
        let player = self.ctx.player.clone();
        player.update(cx, move |p, cx| p.play_context(&label, tracks, ix, cx));
    }

    fn remove_track(&mut self, ix: usize, cx: &mut Context<Self>) {
        match self.kind.clone() {
            ListKind::Playlist(kind) => {
                let Load::Ready(data) = self.data.clone() else { return };
                let Some(track) = data.tracks.get(ix) else { return };
                let (id, album) = (track.id.clone(), track.album_id.clone().unwrap_or_default());
                let api = self.ctx.session.read(cx).yandex();
                let uid = self
                    .ctx
                    .session
                    .read(cx)
                    .profile()
                    .map(|p| p.uid)
                    .unwrap_or(0);
                let weak = cx.entity().downgrade();
                cx.spawn(async move |_, cx| {
                    if api.playlist_delete_track(uid, kind, ix as i64, &id, &album).await.is_ok() {
                        let _ = weak.update(cx, |this, cx| this.load(cx));
                    }
                })
                .detach();
            }
            ListKind::Liked => {
                let Load::Ready(data) = self.data.clone() else { return };
                let Some(track) = data.tracks.get(ix) else { return };
                let api = self.ctx.session.read(cx).yandex();
                let id = track.id.clone();
                let lib = self.ctx.lib.clone();
                lib.update(cx, |lib, cx| lib.toggle_like(&api, &id, false, cx));
                if let Load::Ready(mut d) = self.data.clone() {
                    d.tracks.remove(ix);
                    d.subtitle = format!("{} треков", fmt_count(d.tracks.len() as i64));
                    self.data = Load::Ready(d);
                }
                cx.notify();
            }
            _ => {}
        }
    }

    fn play_all_tracks_for_artist(&mut self, cx: &mut Context<Self>) {
        let ListKind::Artist(id) = self.kind.clone() else { return };
        let api = self.ctx.session.read(cx).yandex();
        let name = match &self.data {
            Load::Ready(d) => d.title.clone(),
            _ => "Артист".to_string(),
        };
        self.loading_tracks = true;
        let player = self.ctx.player.clone();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res = api.artist_tracks(&id).await;
            let _ = weak.update(cx, |this, cx| {
                this.loading_tracks = false;
                if let Ok(tracks) = res {
                    let label = format!("{name} — все треки");
                    let p = player.clone();
                    p.update(cx, |pl, cx| pl.play_context(&label, tracks, 0, cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn ensure_sizes(&mut self, tracks_len: usize, window: &mut Window) {
        let rem = window.rem_size();
        if self.sizes_len == tracks_len && self.sizes_rem == rem {
            return;
        }
        let row = px(self.row_height);
        self.sizes = Rc::new((0..tracks_len).map(|_| size(px(320.0), row)).collect());
        self.sizes_len = tracks_len;
        self.sizes_rem = rem;
    }

    fn render_track(&mut self, ix: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let track = self.tracks_rc[ix].clone();
        let current = self
            .ctx
            .player
            .read(cx)
            .current
            .as_ref()
            .map(|c| c.id.clone())
            == Some(track.id.clone());
        let liked = self.ctx.lib.read(cx).is_liked(&track.id);
        let player = self.ctx.player.clone();
        let label = self.context_label.clone();
        let tracks = Rc::clone(&self.tracks_rc);
        let api = self.ctx.session.read(cx).yandex();
        let lib = self.ctx.lib.clone();
        let view = cx.entity().downgrade();

        let activate: Rc<dyn Fn(usize, &mut Window, &mut gpui_kit::App)> =
            Rc::new(move |ix: usize, _w: &mut Window, cx: &mut gpui_kit::App| {
                let _ = player.update(cx, |p, cx| p.play_context(&label, (*tracks).clone(), ix, cx));
            });

        let mut row = TrackRow::new(track.clone(), ix, activate)
            .liked(liked)
            .current(current)
            .on_like({
                let api = api.clone();
                let lib = lib.clone();
                let id = track.id.clone();
                move |_, _, cx| {
                    lib.update(cx, |lib, cx| lib.toggle_like(&api, &id, false, cx));
                }
            });

        if self.can_remove {
            row = row.trailing({
                Button::new(format!("pl-remove-{}", track.id))
                    .ghost()
                    .small()
                    .icon(IconName::Trash)
                    .tooltip("Убрать из плейлиста")
                    .on_click(move |_, _w, cx| {
                        let _ = view.update(cx, |this, cx| this.remove_track(ix, cx));
                    })
            });
        }
        row
    }

    fn render_track_static(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        // тот же рендер без мутабельности
        let track = self.tracks_rc[ix].clone();
        let current = self
            .ctx
            .player
            .read(cx)
            .current
            .as_ref()
            .map(|c| c.id.clone())
            == Some(track.id.clone());
        let liked = self.ctx.lib.read(cx).is_liked(&track.id);
        let player = self.ctx.player.clone();
        let label = self.context_label.clone();
        let tracks = Rc::clone(&self.tracks_rc);
        let api = self.ctx.session.read(cx).yandex();
        let lib = self.ctx.lib.clone();
        let view = cx.entity().downgrade();
        let can_remove = self.can_remove;

        let activate: Rc<dyn Fn(usize, &mut Window, &mut gpui_kit::App)> =
            Rc::new(move |ix: usize, _w: &mut Window, cx: &mut gpui_kit::App| {
                let _ = player.update(cx, |p, cx| p.play_context(&label, (*tracks).clone(), ix, cx));
            });

        let mut row = TrackRow::new(track.clone(), ix, activate)
            .liked(liked)
            .current(current)
            .on_like({
                let id = track.id.clone();
                move |_, _, cx| {
                    lib.update(cx, |lib, cx| lib.toggle_like(&api, &id, false, cx));
                }
            });
        if can_remove {
            row = row.trailing({
                Button::new(format!("pl-remove-{}", track.id))
                    .ghost()
                    .small()
                    .icon(IconName::Trash)
                    .tooltip("Убрать")
                    .on_click(move |_, _w, cx| {
                        let _ = view.update(cx, |this, cx| this.remove_track(ix, cx));
                    })
            });
        }
        row.into_any_element()
    }

    fn render_header(&self, data: &ListData, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let play_all = {
            let data = data.clone();
            Button::new("list-play")
                .primary()
                .icon(IconName::Play)
                .label("Слушать")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.play_at(0, cx);
                    let _ = &data;
                }))
        };
        let extra = match self.kind {
            ListKind::Artist(_) => Some(
                Button::new("list-artist-all")
                    .outline()
                    .icon(IconName::Music4)
                    .loading(self.loading_tracks)
                    .label("Все треки")
                    .on_click(cx.listener(|this, _, _, cx| this.play_all_tracks_for_artist(cx)))
                    .into_any_element(),
            ),
            ListKind::Playlist(_) | ListKind::Liked => Some(
                Button::new("list-refresh")
                    .ghost()
                    .icon(IconName::RotateCcw)
                    .tooltip("Обновить")
                    .on_click(cx.listener(|this, _, _, cx| this.load(cx)))
                    .into_any_element(),
            ),
            ListKind::Album(_) => None,
        };
        h_flex()
            
            .items_end()
            .gap_6()
            .child(Cover::new(
                data.cover.clone().or_else(|| data.tracks.first().and_then(|t| t.cover_url.clone())),
                160.0,
                "list-cover",
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
                            .child(data.title.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(data.subtitle.clone()),
                    )
                    .child(h_flex().gap_2().pt_2().child(play_all).children(extra)),
            )
    }
}

impl Render for ListPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme().clone();
        let data_opt = match &self.data {
            Load::Ready(d) => Some(d.clone()),
            _ => None,
        };
        let error = match &self.data {
            Load::Failed(msg) => Some(msg.clone()),
            _ => None,
        };

        let body: AnyElement = if let Some(data) = data_opt.clone() {
            self.tracks_rc = Rc::new(data.tracks.clone());
            self.context_label = data.title.clone();
            let tracks_len = data.tracks.len();
            self.ensure_sizes(tracks_len, window);
            let entity = cx.entity();
            let sizes = self.sizes.clone();
            let can_remove = matches!(self.kind, ListKind::Playlist(_) | ListKind::Liked);
            self.can_remove = can_remove;
            let header = self.render_header_inner(&data, cx).into_any_element();
            let albums = if data.albums.is_empty() {
                None
            } else {
                Some(albums_row(&data.albums.clone(), self.ctx.clone(), cx).into_any_element())
            };
            let list: AnyElement = if tracks_len > 30 {
                v_virtual_list(
                    entity,
                    "list-tracks",
                    sizes,
                    move |view: &mut ListPage, range, _window, cx| {
                        range.map(|ix| view.render_track(ix, cx)).collect()
                    },
                )
                .into_any_element()
            } else {
                v_flex()
                    
                    .gap_1()
                    .children((0..tracks_len).map(|ix| self.render_track_static(ix, cx)))
                    .into_any_element()
            };
            v_flex()
                
                .gap_4()
                .child(header)
                .when_some(albums, |this, el| {
                    this.child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Альбомы"),
                            )
                            .child(el),
                    )
                })
                .child(list)
                .into_any_element()
        } else if matches!(self.data, Load::Loading) {
            v_flex()
                
                .items_center()
                .py_12()
                .child(empty_hint("Загружаю…", cx))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        v_flex()
            .id("list-page")
            .size_full()
            .min_w_0()
            .overflow_y_scrollbar()
            .gap_4()
            .p_6()
            .when_some(error, |this, err| this.child(StatusBanner::error(err)))
            .child(body)
    }
}

impl ListPage {
    fn render_header_inner(&self, data: &ListData, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_header(data, cx)
    }
}


fn albums_row(
    albums: &[AlbumBriefDto],
    ctx: PageCtx,
    cx: &mut Context<ListPage>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let app = ctx.app.clone();
    div()
        .id("albums-row")
        
        .gap_3()
        .pb_2()
        .overflow_x_scrollbar()
        .children(albums.iter().take(30).map(|a| {
            let id = a.id.clone();
            let app_i = app.clone();
            div()
                .id(format!("album-mini-{}", a.id))
                .w_40()
                .flex_shrink_0()
                .cursor_pointer()
                .child(Cover::new(a.cover_url.clone(), 160.0, format!("album-c-{}", a.id.clone())))
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
                        .child(a.year.map(|y| y.to_string()).unwrap_or_default()),
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    let id = id.clone();
                    let _ = app_i.update(cx, |this, cx| this.navigate(Route::Album(id), cx));
                }))
        }))
}
