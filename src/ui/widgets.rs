use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::prelude::*;
use gpui_kit::{
    div, img, AnyElement, App, FontWeight, ObjectFit, Pixels, RenderOnce,
    SharedString, Styled, Window, px,
};

use crate::yandex::TrackDto;

pub fn fmt_ms(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    format!("{:02}:{:02}", total / 60, total % 60)
}

pub fn fmt_count(n: i64) -> String {
    if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

/// Обложка фиксированного размера: картинка по URL или заглушка с нотой.
#[derive(IntoElement)]
pub struct Cover {
    url: Option<String>,
    size: Pixels,
    key: SharedString,
}

impl Cover {
    pub fn new(url: Option<String>, size: f32, key: impl Into<SharedString>) -> Self {
        Self {
            url,
            size: px(size),
            key: key.into(),
        }
    }
}

impl RenderOnce for Cover {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let size = self.size;
        let muted_fg = theme.muted_foreground;
        let placeholder = move || {
            div()
                .size(size)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted_fg)
                .child(Icon::new(IconName::Music).small())
                .into_any_element()
        };
        div()
            .id(format!("cover-{}", self.key))
            .size(size)
            .flex_shrink_0()
            .overflow_hidden()
            .rounded(theme.radius)
            .bg(theme.muted)
            .when_some(self.url.clone(), move |this, url| {
                this.child(
                    img(url)
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .with_fallback(placeholder),
                )
            })
            .when(self.url.is_none(), move |this| this.child(placeholder()))
    }
}

/// Строка трека: обложка, название/артист, длительность, лайки, хвост.
#[derive(IntoElement)]
pub struct TrackRow {
    track: TrackDto,
    ix: usize,
    liked: bool,
    disliked: bool,
    current: bool,
    on_activate: Rc<dyn Fn(usize, &mut Window, &mut App)>,
    on_like: Option<Rc<dyn Fn(usize, &mut Window, &mut App)>>,
    on_dislike: Option<Rc<dyn Fn(usize, &mut Window, &mut App)>>,
    trailing: Option<AnyElement>,
}

impl TrackRow {
    pub fn new(
        track: TrackDto,
        ix: usize,
        on_activate: Rc<dyn Fn(usize, &mut Window, &mut App)>,
    ) -> Self {
        Self {
            track,
            ix,
            liked: false,
            disliked: false,
            current: false,
            on_activate,
            on_like: None,
            on_dislike: None,
            trailing: None,
        }
    }

    pub fn liked(mut self, liked: bool) -> Self {
        self.liked = liked;
        self
    }

    pub fn disliked(mut self, disliked: bool) -> Self {
        self.disliked = disliked;
        self
    }

    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    pub fn on_like(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_like = Some(Rc::new(f));
        self
    }

    pub fn on_dislike(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_dislike = Some(Rc::new(f));
        self
    }

    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing = Some(el.into_any_element());
        self
    }
}

impl RenderOnce for TrackRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let radius = theme.radius;
        let c_bg = theme.background;
        let c_accent = theme.accent;
        let c_hover = theme.secondary_hover;
        let c_primary = theme.primary;
        let c_muted_fg = theme.muted_foreground;
        let c_fg = theme.foreground;
        let c_danger = theme.danger;
        let c_border = theme.border;
        let c_warning = theme.warning;
        let ix = self.ix;
        let id = self.track.id.clone();
        let title = self.track.title.clone();
        let artists = self.track.artist_line();
        let cover_small = self.track.cover_url.clone();
        let cover_id = self.track.id.clone();
        let duration = self.track.duration_ms.unwrap_or(0);
        let explicit = self.track.explicit;
        let is_current = self.current;
        let liked = self.liked;
        let disliked = self.disliked;
        let id_like = id.clone();
        let id_dislike = id.clone();
        let on_activate = Rc::clone(&self.on_activate);
        let on_like = self.on_like.clone();
        let on_dislike = self.on_dislike.clone();

        h_flex()
            .id(format!("track-row-{id}"))
            .w_full()
            .h_11()
            .px_3()
            .gap_3()
            .items_center()
            .rounded(radius)
            .cursor_pointer()
            .bg(if is_current { c_accent } else { c_bg })
            .hover(move |s| s.bg(c_hover))
            .on_click(move |_, window, cx| on_activate(ix, window, cx))
            .child(Cover::new(cover_small, 36.0, format!("trow-{cover_id}")))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_weight(if is_current {
                                FontWeight::BOLD
                            } else {
                                FontWeight::SEMIBOLD
                            })
                            .text_color(if is_current { c_primary } else { c_fg })
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(c_muted_fg)
                            .child(artists),
                    ),
            )
            .when(explicit, move |this| {
                this.child(
                    div()
                        .px_1()
                        .rounded(radius)
                        .border_1()
                        .border_color(c_border)
                        .text_xs()
                        .text_color(c_warning)
                        .child("E"),
                )
            })
            .when_some(on_like, move |this, f| {
                this.child(
                    Button::new(format!("like-{id_like}"))
                        .ghost()
                        .xsmall()
                        .icon(if liked {
                            IconName::Heart
                        } else {
                            IconName::HeartOff
                        })
                        .text_color(if liked { c_primary } else { c_muted_fg })
                        .tooltip(if liked { "Убрать лайк" } else { "Лайк" })
                        .on_click(move |_, window, cx| f(ix, window, cx)),
                )
            })
            .when_some(on_dislike, move |this, f| {
                this.child(
                    Button::new(format!("dislike-{id_dislike}"))
                        .ghost()
                        .xsmall()
                        .icon(IconName::ThumbsDown)
                        .text_color(if disliked { c_danger } else { c_muted_fg })
                        .tooltip("Дизлайк")
                        .on_click(move |_, window, cx| f(ix, window, cx)),
                )
            })
            .when_some(self.trailing, |this, el| this.child(el))
            .child(
                div()
                    .text_xs()
                    .text_color(c_muted_fg)
                    .child(fmt_ms(duration)),
            )
    }
}

/// Заголовок секции с опциональной кнопкой справа.
#[derive(IntoElement)]
pub struct SectionHeader {
    title: SharedString,
    action: Option<AnyElement>,
}

impl SectionHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            action: None,
        }
    }

    pub fn action(mut self, el: impl IntoElement) -> Self {
        self.action = Some(el.into_any_element());
        self
    }
}

impl RenderOnce for SectionHeader {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .items_end()
            .justify_between()
            .px_1()
            .pb_1()
            .child(div().text_lg().font_weight(FontWeight::BOLD).child(self.title))
            .when_some(self.action, |this, el| this.child(el))
    }
}

#[derive(IntoElement)]
pub struct StatusBanner {
    text: SharedString,
    error: bool,
}

impl StatusBanner {
    pub fn error(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            error: true,
        }
    }

    pub fn info(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            error: false,
        }
    }
}

impl RenderOnce for StatusBanner {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .w_full()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .text_sm()
            .bg(cx.theme().muted)
            .text_color(if self.error {
                cx.theme().danger
            } else {
                cx.theme().muted_foreground
            })
            .child(self.text)
    }
}

pub fn empty_hint(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .w_full()
        .py_8()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(IconName::Music4).large())
        .child(text.into())
        .into_any_element()
}

/// Строка настройки: подпись и описание слева, контрол справа.
#[derive(IntoElement)]
pub struct SettingRow {
    label: SharedString,
    description: SharedString,
    control: AnyElement,
}

impl SettingRow {
    pub fn new(
        label: impl Into<SharedString>,
        description: impl Into<SharedString>,
        control: impl IntoElement,
    ) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
            control: control.into_any_element(),
        }
    }
}

impl RenderOnce for SettingRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_4()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .bg(cx.theme().secondary)
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(self.label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.description),
                    ),
            )
            .child(self.control)
    }
}

