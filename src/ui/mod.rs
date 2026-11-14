pub mod home;
pub mod library;
pub mod list_page;
pub mod search;
pub mod settings;
pub mod wave;
pub mod widgets;

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Selectable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    notification::Notification,
    scroll::ScrollableElement,
    slider::{Slider, SliderEvent, SliderState},
    h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::lib_index::{LibEvent, LibIndex};
use crate::player::Player;
use crate::session::{Session, SessionEvent};
use crate::store::{Settings, Store};
use crate::yandex::LyricsDto;

use home::HomePage;
use library::LibraryPage;
use list_page::ListPage;
use search::SearchPage;
use settings::SettingsPage;
use wave::WavePage;
use widgets::{Cover, fmt_count, fmt_ms};

pub const MY_WAVE_STATION: &str = "radio-user-default";

actions!(
    meowsic,
    [TogglePlay, SkipNext, SkipPrev, ToggleLyrics, OpenQueue, OpenSearch]
);

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Route {
    Home,
    Wave,
    Search,
    Library,
    Liked,
    Settings,
    Playlist(i64),
    Album(String),
    Artist(String),
}

pub enum Page {
    Home(Entity<HomePage>),
    Wave(Entity<WavePage>),
    Search(Entity<SearchPage>),
    Library(Entity<LibraryPage>),
    Settings(Entity<SettingsPage>),
    List(Entity<ListPage>),
}

/// Всё, что нужно страницам: общие модели и слабая ручка на оболочку.
#[derive(Clone)]
pub struct PageCtx {
    pub session: Entity<Session>,
    pub player: Entity<Player>,
    pub lib: Entity<LibIndex>,
    pub app: WeakEntity<MusicApp>,
}

#[derive(Clone)]
pub enum Load<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(SharedString),
}

pub struct MusicApp {
    pub store: Entity<Store>,
    pub settings: Settings,
    pub session: Entity<Session>,
    pub player: Entity<Player>,
    pub lib: Entity<LibIndex>,
    pub route: Route,
    page: Page,
    home: Option<Entity<HomePage>>,
    pub wave: Entity<WavePage>,
    pub search: Entity<SearchPage>,
    pub library: Entity<LibraryPage>,
    pub settings_page: Entity<SettingsPage>,

    pub seek: Entity<SliderState>,
    pub volume: Entity<SliderState>,
    seek_dragging: bool,
    seek_preview_ms: Option<i64>,

    pub lyrics_open: bool,
    pub lyrics: Load<LyricsDto>,
    lyrics_track: Option<String>,
    lyrics_active: Option<usize>,

    focus: FocusHandle,
    on_player_state_pending: bool,
    _subs: Vec<Subscription>,
}

impl MusicApp {
    pub fn new(window: &mut Window, store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let settings = store.read(cx).settings.clone();

        cx.bind_keys([
            KeyBinding::new("space", TogglePlay, Some("meowsic")),
            KeyBinding::new("ctrl-right", SkipNext, Some("meowsic")),
            KeyBinding::new("ctrl-left", SkipPrev, Some("meowsic")),
            KeyBinding::new("ctrl-l", ToggleLyrics, Some("meowsic")),
            KeyBinding::new("ctrl-q", OpenQueue, Some("meowsic")),
            KeyBinding::new("f11", OpenSearch, Some("meowsic")),
        ]);

        let session = cx.new(|cx| Session::new(&store, cx));
        let api = session.read(cx).api_handle();
        let player = cx.new(|cx| Player::new(api, store.clone(), cx));
        let lib = cx.new(|_| LibIndex::new());
        let weak = cx.entity().downgrade();
        let ctx = PageCtx {
            session: session.clone(),
            player: player.clone(),
            lib: lib.clone(),
            app: weak,
        };

        let seek = cx.new(|_| SliderState::new().min(0.0).max(1.0).default_value(0.0));
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .default_value(settings.volume)
        });
        let home = cx.new(|cx| HomePage::new(ctx.clone(), cx));

        let mut this = Self {
            store: store.clone(),
            settings: settings.clone(),
            session,
            player,
            lib,
            route: Route::Home,
            page: Page::Home(home.clone()),
            home: Some(home),
            wave: cx.new(|cx| WavePage::new(ctx.clone(), cx)),
            search: cx.new(|cx| SearchPage::new(ctx.clone(), window, cx)),
            library: cx.new(|cx| LibraryPage::new(ctx.clone(), cx)),
            settings_page: cx.new(|cx| SettingsPage::new(ctx.clone(), settings.clone(), window, cx)),
            seek,
            volume,
            seek_dragging: false,
            seek_preview_ms: None,
            lyrics_open: false,
            lyrics: Load::Idle,
            lyrics_track: None,
            lyrics_active: None,
            focus: cx.focus_handle(),
            on_player_state_pending: false,
            _subs: Vec::new(),
        };
        window.focus(&this.focus, cx);
        this.wire_events(window, cx);

        if settings.restore_session {
            let saved = this.store.read(cx).load_session();
            if let Some(saved) = saved {
                let autoplay = settings.autoplay_restored;
                this.player
                    .update(cx, |player, cx| player.restore(saved, autoplay, cx));
            }
        }
        let token = settings.token.clone();
        if !token.is_empty() {
            this.session.update(cx, |s, cx| s.login(token, cx));
        }
        this
    }

    fn wire_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let player = self.player.clone();
        self._subs
            .push(cx.observe(&player, |this, _, cx| {
                this.on_player_state_pending = true;
                let _ = this;
                cx.notify();
            }));

        let lib = self.lib.clone();
        self._subs.push(cx.subscribe_in(&lib, window, |this, _, event, window, cx| {
            match event {
                LibEvent::Changed => {
                    let _ = this;
                    cx.notify()
                }
                LibEvent::Error(msg) => {
                    window.push_notification(Notification::error(msg.clone()), cx);
                }
            }
        }));

        let session = self.session.clone();
        self._subs
            .push(cx.subscribe_in(&session, window, |this, _, event, window, cx| {
                match event {
                    SessionEvent::LoggedIn => this.on_logged_in(cx),
                    SessionEvent::LoggedOut => {
                        this.lib.update(cx, |lib, cx| lib.set_uid(0, cx));
                        this.player.update(cx, |p, _| p.set_uid(0));
                        this.settings.token.clear();
                        self_persist(this, cx);
                        cx.notify();
                    }
                    SessionEvent::LoginFailed(msg) => {
                        window.push_notification(Notification::error(msg.clone()), cx);
                        this.settings_page
                            .update(cx, |page, cx| page.set_login_error(msg.clone(), cx));
                    }
                }
            }));

        let seek = self.seek.clone();
        self._subs.push(cx.subscribe_in(&seek, window, |this, _s, event, _w, cx| {
            match event {
                SliderEvent::Change(v) => {
                    this.seek_dragging = true;
                    let dur = this.player.read(cx).duration_ms().max(1) as f32;
                    this.seek_preview_ms = Some((v.start() * dur) as i64);
                }
                SliderEvent::Release(v) => {
                    this.seek_dragging = false;
                    this.seek_preview_ms = None;
                    let dur = this.player.read(cx).duration_ms().max(1) as f32;
                    let ms = (v.start() * dur).clamp(0.0, dur) as i64;
                    this.player.update(cx, |p, cx| p.seek(ms, cx));
                }
            }
        }));

        let volume = self.volume.clone();
        self._subs.push(cx.subscribe_in(&volume, window, |this, _s, event, _w, cx| {
            let v = match event {
                SliderEvent::Change(v) | SliderEvent::Release(v) => *v,
            };
            {
                let vol = v.start().clamp(0.0, 1.0);
                this.player.update(cx, |p, cx| p.set_volume(vol, cx));
                this.settings.volume = vol;
                let s = this.settings.clone();
                this.store.update(cx, |store, _| store.save_settings(&s));
            }
        }));
    }

    pub fn sync_from_player(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_player_state(window, cx);
    }

    fn on_player_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (pos, dur) = {
            let p = self.player.read(cx);
            (p.position_ms(), p.duration_ms())
        };
        if !self.seek_dragging {
            let value = if dur > 0 {
                (pos.max(0) as f32 / dur as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.seek.update(cx, |state, cx| {
                state.set_value(value, window, cx);
            });
        }

        if self.lyrics_open {
            let track_changed = {
                let cur = self.player.read(cx).current.as_ref().map(|t| t.id.clone());
                cur != self.lyrics_track
            };
            if track_changed {
                self.refresh_lyrics(cx);
            }
            if let Load::Ready(lyrics) = self.lyrics.clone() {
                if lyrics.synced && !lyrics.lines.is_empty() {
                    let pos = self.player.read(cx).position_ms();
                    let ix = lyrics
                        .lines
                        .iter()
                        .rposition(|l| l.time_ms <= pos)
                        .unwrap_or(0);
                    self.lyrics_active = Some(ix);
                }
            }
        }
        cx.notify();
    }

    fn on_logged_in(&mut self, cx: &mut Context<Self>) {
        let uid = self
            .session
            .read(cx)
            .profile()
            .map(|p| p.uid)
            .unwrap_or(0);
        let api = self.session.read(cx).yandex();
        self.player.update(cx, |p, _| p.set_uid(uid));
        self.lib.update(cx, |lib, cx| {
            lib.set_uid(uid, cx);
            lib.refresh(&api, cx);
        });
        self.settings.token = api.token();
        self_persist(self, cx);
        if let Some(home) = self.home.clone() {
            home.update(cx, |page, cx| page.reload(cx));
        }
        cx.notify();
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        f(&mut self.settings);
        let s = self.settings.clone();
        self.player.update(cx, |p, cx| {
            p.set_quality(s.quality, cx);
            p.set_eq(s.eq.enabled, s.eq.gains_db.clone(), cx);
        });
        self.store.update(cx, |store, _| store.save_settings(&s));
        self.settings_page
            .update(cx, |page, cx| page.set_settings(s.clone(), cx));
        cx.notify();
    }

    pub fn apply_theme(&self, cx: &mut App) {
        use gpui_kit::component::{Theme, ThemeMode};
        let mode = if self.settings.theme_dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(mode, None, cx);
        if let Some(hex) = self.settings.accent.clone() {
            if let Some(color) = parse_hex(&hex) {
                Theme::update(cx, |theme| theme.primary = color);
            }
        }
    }

    pub fn persist_settings(&mut self, cx: &mut Context<Self>) {
        self_persist(self, cx);
    }

    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        let ctx = self.page_ctx(cx);
        self.page = match &route {
            Route::Home => Page::Home(
                self.home
                    .clone()
                    .unwrap_or_else(|| cx.new(|cx| HomePage::new(ctx.clone(), cx))),
            ),
            Route::Wave => Page::Wave(self.wave.clone()),
            Route::Search => Page::Search(self.search.clone()),
            Route::Library => Page::Library(self.library.clone()),
            Route::Settings => Page::Settings(self.settings_page.clone()),
            Route::Liked => Page::List(cx.new(|cx| ListPage::liked(ctx.clone(), cx))),
            Route::Playlist(kind) => {
                let kind = *kind;
                Page::List(cx.new(|cx| ListPage::playlist(kind, ctx.clone(), cx)))
            }
            Route::Album(id) => {
                let id = id.clone();
                Page::List(cx.new(|cx| ListPage::album(id, ctx.clone(), cx)))
            }
            Route::Artist(id) => {
                let id = id.clone();
                Page::List(cx.new(|cx| ListPage::artist(id, ctx.clone(), cx)))
            }
        };
        if let Page::Home(e) = &self.page {
            self.home = Some(e.clone());
        }
        self.route = route;
        cx.notify();
    }

    pub fn page_ctx(&self, cx: &mut Context<Self>) -> PageCtx {
        PageCtx {
            session: self.session.clone(),
            player: self.player.clone(),
            lib: self.lib.clone(),
            app: cx.entity().downgrade(),
        }
    }

    pub fn toast(msg: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
        window.push_notification(Notification::info(msg.into()), cx);
    }

    pub fn start_wave(&mut self, cx: &mut Context<Self>) {
        self.player
            .update(cx, |p, cx| p.start_wave(MY_WAVE_STATION, "Моя волна", cx));
        self.navigate(Route::Wave, cx);
    }

    pub fn toggle_lyrics(&mut self, cx: &mut Context<Self>) {
        self.lyrics_open = !self.lyrics_open;
        if self.lyrics_open {
            self.refresh_lyrics(cx);
        }
        cx.notify();
    }

    fn refresh_lyrics(&mut self, cx: &mut Context<Self>) {
        let Some(track) = self.player.read(cx).current.clone() else {
            self.lyrics = Load::Idle;
            self.lyrics_track = None;
            return;
        };
        if self.lyrics_track.as_deref() == Some(track.id.as_str())
            && matches!(self.lyrics, Load::Ready(_) | Load::Loading)
        {
            return;
        }
        self.lyrics_track = Some(track.id.clone());
        self.lyrics = Load::Loading;
        self.lyrics_active = None;
        let api = self.session.read(cx).yandex();
        let expect = track.id.clone();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res = api.lyrics_lrclib(&track).await;
            let _ = weak.update(cx, |this, cx| {
                if this.lyrics_track.as_deref() != Some(expect.as_str()) {
                    return;
                }
                this.lyrics = match res {
                    Ok(lyrics) if !lyrics.lines.is_empty() => Load::Ready(lyrics),
                    Ok(_) => Load::Failed("Для этого трека текста нет".into()),
                    Err(e) => Load::Failed(e.to_string().into()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn open_queue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let player = self.player.clone();
        window.open_sheet(cx, move |sheet, _window, _cx| {
            sheet
                .title("Очередь")
                .child(QueueList::new(player.clone()))
        });
    }

    // --- действия ---

    fn dispatch_toggle_play(&mut self, _: &TogglePlay, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_focused_input(cx) {
            return;
        }
        self.player.update(cx, |p, cx| p.toggle_play(cx));
    }

    fn dispatch_next(&mut self, _: &SkipNext, _: &mut Window, cx: &mut Context<Self>) {
        self.player.update(cx, |p, cx| p.next(cx));
    }

    fn dispatch_prev(&mut self, _: &SkipPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.player.update(cx, |p, cx| p.prev(cx));
    }

    fn dispatch_lyrics(&mut self, _: &ToggleLyrics, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_lyrics(cx);
    }

    fn dispatch_queue(&mut self, _: &OpenQueue, window: &mut Window, cx: &mut Context<Self>) {
        self.open_queue(window, cx);
    }

    fn dispatch_search(&mut self, _: &OpenSearch, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Route::Search, cx);
    }

    // --- рендер ---

    fn page_element(&self, _cx: &App) -> AnyElement {
        match &self.page {
            Page::Home(e) => e.clone().into_any_element(),
            Page::Wave(e) => e.clone().into_any_element(),
            Page::Search(e) => e.clone().into_any_element(),
            Page::Library(e) => e.clone().into_any_element(),
            Page::Settings(e) => e.clone().into_any_element(),
            Page::List(e) => e.clone().into_any_element(),
        }
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let route = self.route.clone();
        let logged = self.session.read(cx).is_logged_in();
        let profile = self.session.read(cx).profile().cloned();
        let playlists = self.lib.read(cx).playlists.clone();

        v_flex()
            .w_56()
            .h_full()
            .flex_shrink_0()
            .gap_1()
            .p_3()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .px_1()
                    .pb_2()
                    .child(
                        div()
                            .size_6()
                            .rounded_full()
                            .bg(theme.primary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(theme.primary_foreground)
                            .child(Icon::new(IconName::Music).xsmall()),
                    )
                    .child(div().text_lg().font_weight(FontWeight::BOLD).child("meowsic")),
            )
            .child(nav_item("Главная", IconName::House, Route::Home, &route, cx))
            .child(nav_item("Моя волна", IconName::Radio, Route::Wave, &route, cx))
            .child(nav_item("Поиск", IconName::Search, Route::Search, &route, cx))
            .child(nav_item("Библиотека", IconName::Library, Route::Library, &route, cx))
            .child(nav_item("Мне нравится", IconName::Heart, Route::Liked, &route, cx))
            .when(logged, |this| {
                this.child(
                    div()
                        .mt_2()
                        .px_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Плейлисты"),
                )
            })
            .child(
                v_flex()
                    .id("sidebar-playlists")
                    .min_h_0()
                    .flex_1()
                    .gap_0()
                    .overflow_y_scrollbar()
                    .children(playlists.iter().take(30).map(|p| {
                        let kind = p.kind;
                        let title = p.title.clone();
                        let cover = p.cover_url.clone();
                        let r = Route::Playlist(kind);
                        let active = route == r;
                        h_flex()
                            .id(format!("pl-{kind}"))
                            .w_full()
                            .h_8()
                            .px_2()
                            .gap_2()
                            .items_center()
                            .rounded(theme.radius)
                            .cursor_pointer()
                            .bg(if active { theme.accent } else { theme.background })
                            .hover(|s| s.bg(theme.secondary_hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.navigate(r.clone(), cx)
                            }))
                            .when(self.settings.sidebar_covers, |this| {
                                this.child(Cover::new(cover, 20.0, format!("plc-{kind}")))
                            })
                            .child(div().truncate().text_sm().child(title))
                    })),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .px_1()
                    .pt_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(Cover::new(
                        profile.as_ref().and_then(|p| p.avatar_url.clone()),
                        28.0,
                        "avatar",
                    ))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div().truncate().text_sm().child(
                                    profile
                                        .as_ref()
                                        .map(|p| p.display_name.clone())
                                        .unwrap_or_else(|| "Не войден".to_string()),
                                ),
                            )
                            .when_some(
                                profile.as_ref().filter(|p| p.has_plus).map(|_| "Плюс"),
                                |this, plus| {
                                    this.child(div().text_xs().text_color(theme.primary).child(plus))
                                },
                            ),
                    )
                    .child(
                        Button::new("nav-settings")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Settings)
                            .tooltip("Настройки")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.navigate(Route::Settings, cx)
                            })),
                    ),
            )
    }

    fn render_player_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (current, playing, loading, duration, position, shuffle, repeat, context, error, wave_active) = {
            let p = self.player.read(cx);
            (
                p.current.clone(),
                p.playing,
                p.loading,
                p.duration_ms(),
                p.position_ms(),
                p.shuffle,
                p.repeat,
                p.context_name.clone(),
                p.error.clone(),
                p.wave.is_some(),
            )
        };
        let shown_position = self.seek_preview_ms.unwrap_or(position);
        let volume = self.settings.volume;
        let track_block = current.as_ref().map(|track| {
            let liked = self.lib.read(cx).is_liked(&track.id);
            let disliked = self.lib.read(cx).is_disliked(&track.id);
            let api = self.session.read(cx).yandex();
            h_flex()
                .gap_1()
                .child(like_button(
                    format!("bar-like-{}", track.id),
                    "Лайк",
                    liked,
                    wave_active,
                    theme.primary,
                    theme.muted_foreground,
                    track.id.clone(),
                    api.clone(),
                    self.lib.clone(),
                    false,
                ))
                .child(like_button(
                    format!("bar-dislike-{}", track.id),
                    "Дизлайк",
                    disliked,
                    wave_active,
                    theme.danger,
                    theme.muted_foreground,
                    track.id.clone(),
                    api,
                    self.lib.clone(),
                    true,
                ))
                .into_any_element()
        });

        h_flex()
            .w_full()
            .h_24()
            .flex_shrink_0()
            .px_4()
            .gap_4()
            .items_center()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            // левый блок: обложка + метаданные + лайки
            .child(
                h_flex()
                    .w_72()
                    .flex_shrink_0()
                    .gap_3()
                    .items_center()
                    .child(Cover::new(
                        current.as_ref().and_then(|t| t.cover_url.clone()),
                        56.0,
                        "now-cover",
                    ))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(
                                        current
                                            .as_ref()
                                            .map(|t| t.title.clone())
                                            .unwrap_or_else(|| context.to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        current
                                            .as_ref()
                                            .map(|t| t.artist_line())
                                            .unwrap_or_else(|| "—".to_string()),
                                    ),
                            ),
                    )
                    .when_some(track_block, |this, el| this.child(el)),
            )
            // центр: транспорт и прогресс
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("shuffle")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Shuffle)
                                    .text_color(if shuffle {
                                        theme.primary
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .tooltip(if shuffle {
                                        "Перемешивание: вкл"
                                    } else {
                                        "Перемешивание: выкл"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let on = !this.player.read(cx).shuffle;
                                        this.player.update(cx, |p, cx| p.set_shuffle(on, cx));
                                    })),
                            )
                            .child(
                                Button::new("prev")
                                    .ghost()
                                    .small()
                                    .icon(IconName::SkipBack)
                                    .tooltip("Назад (Ctrl-←)")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |p, cx| p.prev(cx));
                                    })),
                            )
                            .child(
                                Button::new("play")
                                    .primary()
                                    .large()
                                    .loading(loading)
                                    .icon(if playing {
                                        IconName::Pause
                                    } else {
                                        IconName::Play
                                    })
                                    .tooltip(if playing { "Пауза (Space)" } else { "Играть (Space)" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |p, cx| p.toggle_play(cx));
                                    })),
                            )
                            .child(
                                Button::new("next")
                                    .ghost()
                                    .small()
                                    .icon(IconName::SkipForward)
                                    .tooltip("Вперёд (Ctrl-→)")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |p, cx| p.next(cx));
                                    })),
                            )
                            .child({
                                let (icon, tip, on) = match repeat {
                                    crate::player::RepeatMode::Off => {
                                        (IconName::Repeat, "Повтор выключен", false)
                                    }
                                    crate::player::RepeatMode::All => {
                                        (IconName::Repeat, "Повтор очереди", true)
                                    }
                                    crate::player::RepeatMode::One => {
                                        (IconName::Repeat1, "Повтор трека", true)
                                    }
                                };
                                Button::new("repeat")
                                    .ghost()
                                    .small()
                                    .icon(icon)
                                    .text_color(if on {
                                        theme.primary
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .tooltip(tip)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |p, cx| p.cycle_repeat(cx));
                                    }))
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .w_12()
                                    .child(fmt_ms(shown_position)),
                            )
                            .child(div().flex_1().min_w_0().child(Slider::new(&self.seek)))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .w_12()
                                    .text_right()
                                    .child(fmt_ms(duration)),
                            ),
                    ),
            )
            // правый блок: громкость, тексты, очередь
            .child(
                h_flex()
                    .w_64()
                    .flex_shrink_0()
                    .gap_1()
                    .justify_end()
                    .items_center()
                    .child(
                        Icon::new(if volume < 0.01 {
                            IconName::VolumeX
                        } else if volume < 0.6 {
                            IconName::Volume1
                        } else {
                            IconName::Volume2
                        })
                        .small()
                        .text_color(theme.muted_foreground),
                    )
                    .child(div().w_24().child(Slider::new(&self.volume)))
                    .child(
                        Button::new("bar-lyrics")
                            .ghost()
                            .small()
                            .icon(IconName::MicVocal)
                            .text_color(if self.lyrics_open {
                                theme.primary
                            } else {
                                theme.muted_foreground
                            })
                            .tooltip("Текст песни (Ctrl-L)")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_lyrics(cx))),
                    )
                    .child(
                        Button::new("bar-queue")
                            .ghost()
                            .small()
                            .icon(IconName::ListMusic)
                            .tooltip("Очередь (Ctrl-Q)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_queue(window, cx)
                            })),
                    )
                    .child(
                        Button::new("bar-add")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .tooltip("Добавить в плейлист")
                            .on_click(cx.listener(|this, _, window, cx| {
                                let lib = this.lib.clone();
                                let player = this.player.clone();
                                let session = this.session.clone();
                                window.open_sheet(cx, move |sheet, _w, _cx| {
                                    sheet.title("Добавить в плейлист").child(PlaylistPicker::new(
                                        lib.clone(),
                                        player.clone(),
                                        session.clone(),
                                    ))
                                });
                            })),
                    ),
            )
            .when_some(error, |this, err| {
                this.child(
                    div()
                        .text_xs()
                        .max_w_64()
                        .truncate()
                        .text_color(theme.danger)
                        .child(err),
                )
            })
    }

    fn render_lyrics_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        match self.lyrics.clone() {
            Load::Idle => centered_hint("Включи трек — текст появится здесь", cx),
            Load::Loading => centered_hint("Ищу текст…", cx),
            Load::Failed(msg) => centered_hint(msg, cx),
            Load::Ready(lyrics) => {
                let active = self.lyrics_active.unwrap_or(0);
                let range = if lyrics.synced {
                    let start = active.saturating_sub(12);
                    start..lyrics.lines.len().min(start + 24)
                } else {
                    0..lyrics.lines.len().min(120)
                };
                let track_id = self.lyrics_track.clone().unwrap_or_default();
                let player = self.player.clone();
                let range_start = range.start;
                let lines: Vec<crate::yandex::LyricsLineDto> = lyrics.lines[range].to_vec();
                let synced = lyrics.synced;
                div()
                    .id("lyrics-scroll")
                    .size_full()
                    .overflow_y_scrollbar()
                    .child(
                        v_flex()
                            .gap_2()
                            .p_4()
                            .children(lines.iter().enumerate().map(|(offset, line)| {
                                let absolute = if synced {
                                    range_start + offset
                                } else {
                                    offset
                                };
                                let is_active = synced && absolute == active;
                                let time = line.time_ms;
                                let p = player.clone();
                                let tid = track_id.clone();
                                div()
                                    .id(format!("lyrics-line-{tid}-{absolute}"))
                                    .w_full()
                                    .text_base()
                                    .when(synced, move |this| {
                                        this.cursor_pointer().on_click(move |_, _, cx| {
                                            p.update(cx, |pl, cx| pl.seek(time, cx));
                                        })
                                    })
                                    .text_color(if is_active {
                                        theme.primary
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .font_weight(if is_active {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(line.text.clone())
                            })),
                    )
                    .into_any_element()
            }
        }
    }
}

fn self_persist(this: &mut MusicApp, cx: &mut Context<MusicApp>) {
    let s = this.settings.clone();
    this.store.update(cx, |store, _| store.save_settings(&s));
}

impl Render for MusicApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.on_player_state_pending) {
            self.sync_from_player(window, cx);
        }
        let theme = cx.theme().clone();
        let page = self.page_element(cx);
        let lyrics_open = self.lyrics_open;
        v_flex()
            .id("meowsic-root")
            .size_full()
            .key_context("meowsic")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::dispatch_toggle_play))
            .on_action(cx.listener(Self::dispatch_next))
            .on_action(cx.listener(Self::dispatch_prev))
            .on_action(cx.listener(Self::dispatch_lyrics))
            .on_action(cx.listener(Self::dispatch_queue))
            .on_action(cx.listener(Self::dispatch_search))
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                h_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(self.render_sidebar(cx))
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_stretch()
                            .child(div().id("page").flex_1().min_w_0().child(page))
                            .when(lyrics_open, |this| {
                                this.child(
                                    div()
                                        .w_80()
                                        .h_full()
                                        .flex_shrink_0()
                                        .border_l_1()
                                        .border_color(theme.border)
                                        .child(self.render_lyrics_panel(cx)),
                                )
                            }),
                    ),
            )
            .child(self.render_player_bar(cx))
    }
}

fn nav_item(
    label: &'static str,
    icon: IconName,
    route: Route,
    current: &Route,
    cx: &mut Context<MusicApp>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let active = current == &route;
    Button::new(format!("nav-{label}"))
        .ghost()
        .w_full()
        .selected(active)
        .text_color(if active { theme.primary } else { theme.foreground })
        .icon(icon)
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| this.navigate(route.clone(), cx)))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn like_button(
    id: String,
    tip: &'static str,
    on: bool,
    wave: bool,
    accent: Hsla,
    muted: Hsla,
    track_id: String,
    api: crate::yandex::Yandex,
    lib: Entity<LibIndex>,
    dislike: bool,
) -> AnyElement {
    let _ = wave;
    Button::new(id)
        .ghost()
        .small()
        .icon(if dislike {
            IconName::ThumbsDown
        } else if on {
            IconName::Heart
        } else {
            IconName::HeartOff
        })
        .text_color(if on { accent } else { muted })
        .tooltip(tip)
        .on_click(move |_, _, cx| {
            let id = track_id.clone();
            lib.update(cx, |lib, cx| lib.toggle_like(&api, &id, dislike, cx));
        })
        .into_any_element()
}

fn centered_hint(msg: impl Into<SharedString>, cx: &App) -> AnyElement {
    let msg = msg.into();
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .p_6()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(IconName::Music4).large())
        .child(msg)
        .into_any_element()
}

pub fn parse_hex(hex: &str) -> Option<Hsla> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    Some(Rgba { r, g, b, a: 1.0 }.into())
}

#[derive(IntoElement)]
struct PlaylistPicker {
    lib: Entity<LibIndex>,
    player: Entity<Player>,
    session: Entity<Session>,
}

impl PlaylistPicker {
    fn new(lib: Entity<LibIndex>, player: Entity<Player>, session: Entity<Session>) -> Self {
        Self { lib, player, session }
    }
}

impl RenderOnce for PlaylistPicker {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current = self.player.read(cx).current.clone();
        let Some(track) = current else {
            return centered_hint("Сначала включи трек", cx);
        };
        let playlists = self.lib.read(cx).playlists.clone();
        let uid = self.session.read(cx).profile().map(|p| p.uid).unwrap_or(0);
        let api = self.session.read(cx).yandex();
        let track_id = track.id.clone();
        let album_id = track.album_id.clone().unwrap_or_default();
        let lib = self.lib.clone();

        v_flex()
            .id("picker")
            .w_full()
            .gap_1()
            .p_2()
            .children(playlists.into_iter().map(|pl| {
                let api = api.clone();
                let lib_row = lib.clone();
                let track_id_row = track_id.clone();
                let album_id_row = album_id.clone();
                let at = pl.track_count.max(0) + 1;
                h_flex()
                    .id(format!("pick-{}", pl.kind))
                    .w_full()
                    .h_12()
                    .px_3()
                    .gap_3()
                    .items_center()
                    .cursor_pointer()
                    .rounded(theme.radius)
                    .hover(|s| s.bg(theme.secondary_hover))
                    .child(Cover::new(pl.cover_url.clone(), 32.0, format!("pickc-{}", pl.kind)))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(pl.title.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{} треков", fmt_count(pl.track_count))),
                            ),
                    )
                    .on_click(move |_, window, cx| {
                        let api = api.clone();
                        let lib = lib_row.clone();
                        let track_id = track_id_row.clone();
                        let album_id = album_id_row.clone();
                        let kind = pl.kind;
                        cx.spawn(async move |cx| {
                            match api.playlist_insert(uid, kind, at, &track_id, &album_id).await {
                                Ok(_) => {
                                    let _ = lib.update(cx, |lib, cx| {
                                        let mut list = lib.playlists.clone();
                                        if let Some(p) = list.iter_mut().find(|p| p.kind == kind) {
                                            p.track_count += 1;
                                        }
                                        lib.replace_playlists(list, cx);
                                    });
                                }
                                Err(e) => {
                                    let _ =
                                        lib.update(cx, |lib, cx| lib.report_error(&e.to_string(), cx));
                                }
                            }
                        })
                        .detach();
                        window.close_sheet(cx);
                    })
            }))
            .into_any_element()
    }
}


#[derive(IntoElement)]
struct QueueList {
    player: Entity<Player>,
}

impl QueueList {
    fn new(player: Entity<Player>) -> Self {
        Self { player }
    }
}

impl RenderOnce for QueueList {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let items = self.player.read(cx).snapshot_queue();
        let cur = self.player.read(cx).current_order_ix();
        let player = self.player.clone();
        v_flex()
            .id("queue-list")
            .w_full()
            .min_h_0()
            .flex_1()
            .gap_1()
            .p_2()
            .overflow_y_scrollbar()
            .children(items.into_iter().enumerate().map(|(ix, item)| {
                let p = player.clone();
                let title = item.track.title.clone();
                let artists = item.track.artist_line();
                let cover = item.track.cover_url.clone();
                let tid = item.track.id.clone();
                h_flex()
                    .id(format!("queue-{ix}"))
                    .w_full()
                    .h_10()
                    .px_2()
                    .gap_2()
                    .items_center()
                    .rounded(theme.radius)
                    .bg(if ix == cur { theme.accent } else { theme.background })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.secondary_hover))
                    .on_click(move |_, _, cx| {
                        p.update(cx, |pl, cx| pl.play_at_order_ix(ix, cx));
                    })
                    .child(Cover::new(cover, 28.0, format!("qc-{tid}")))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .child(div().truncate().text_sm().child(title))
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(artists),
                            ),
                    )
            }))
    }
}

// хелпер-реэкспорт для страниц
