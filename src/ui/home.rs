use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    h_flex, v_flex,
};

use gpui_kit::{AnyElement, Context, SharedString, Subscription, Window, div};
use gpui_kit::prelude::*;
use crate::session::SessionEvent;
use crate::ui::widgets::{Cover, StatusBanner, empty_hint};
use crate::yandex::WheelItemDto;
use super::Load;
use super::MY_WAVE_STATION;
use super::PageCtx;
use super::Route;

pub struct HomePage {
    ctx: PageCtx,
    wheel: Load<Vec<WheelItemDto>>,
    _subs: Vec<Subscription>,
}

impl HomePage {
    pub fn new(ctx: PageCtx, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            ctx,
            wheel: Load::Idle,
            _subs: Vec::new(),
        };
        let session = this.ctx.session.clone();
        this._subs.push(cx.subscribe(&session, |this, _, event, cx| {
            if let SessionEvent::LoggedIn = event {
                this.reload(cx);
            }
        }));
        if session.read(cx).is_logged_in() {
            this.reload(cx);
        }
        this
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if !self.ctx.session.read(cx).is_logged_in() {
            self.wheel = Load::Idle;
            cx.notify();
            return;
        }
        self.wheel = Load::Loading;
        let api = self.ctx.session.read(cx).yandex();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res = api.wheel().await;
            let _ = weak.update(cx, |this, cx| {
                this.wheel = match res {
                    Ok(items) if items.is_empty() => Load::Failed("Колесо пустое".into()),
                    Ok(items) => Load::Ready(items),
                    Err(e) => Load::Failed(e.to_string().into()),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_item(&mut self, item: &WheelItemDto, cx: &mut Context<Self>) {
        let (kind, id, name, station) = (
            item.kind.clone(),
            item.id.clone(),
            item.name.clone(),
            item.station.clone(),
        );
        let player = self.ctx.player.clone();
        let app = self.ctx.app.clone();
        match kind.as_str() {
            "wave" => {
                let station = station.clone().unwrap_or_else(|| MY_WAVE_STATION.to_string());
                player.update(cx, |p, cx| p.start_wave(&station, &name, cx));
                let _ = app.update(cx, |this, cx| this.navigate(Route::Wave, cx));
            }
            "album" => {
                let _ = app.update(cx, |this, cx| this.navigate(Route::Album(id), cx));
            }
            "artist" => {
                let _ = app.update(cx, |this, cx| this.navigate(Route::Artist(id), cx));
            }
            "playlist" => {
                if let Ok(kind) = id.parse::<i64>() {
                    let _ = app.update(cx, |this, cx| this.navigate(Route::Playlist(kind), cx));
                }
            }
            _ => {}
        }
    }
}

impl Render for HomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let logged = self.ctx.session.read(cx).is_logged_in();
        let display_name: SharedString = self
            .ctx
            .session
            .read(cx)
            .profile()
            .map(|p| p.display_name.clone())
            .unwrap_or_default()
            .into();

        v_flex()
            .id("home")
            .size_full()
            .min_w_0()
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .gap_6()
                    .p_6()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(gpui_kit::FontWeight::BOLD)
                                    .child(if logged {
                                        format!("Привет, {display_name}")
                                    } else {
                                        "meowsic".to_string()
                                    }),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(if logged {
                                        "Твоя волна ждёт"
                                    } else {
                                        "Введи токен Яндекса в настройках — и заработает вся магия"
                                    }),
                            ),
                    )
                    .child(self.render_wave_hero(cx))
                    .child(self.render_wheel(cx)),
            )
    }
}

impl HomePage {
    fn render_wave_hero(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let player = self.ctx.player.clone();
        let app = self.ctx.app.clone();
        h_flex()
            
            .h_40()
            .items_center()
            .justify_between()
            .px_6()
            .rounded(theme.radius)
            .bg(theme.accent)
            .border_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui_kit::FontWeight::BOLD)
                            .child("Моя волна"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Персональный поток, подстраивается под лайки и дизлайки"),
                    ),
            )
            .child(
                Button::new("home-wave")
                    .primary()
                    .large()
                    .icon(gpui_kit::component::IconName::Play)
                    .label("Запустить")
                    .on_click(move |_, _, cx| {
                        player.update(cx, |p, cx| p.start_wave(MY_WAVE_STATION, "Моя волна", cx));
                        let _ = app.update(cx, |this, cx| this.navigate(Route::Wave, cx));
                    }),
            )
    }

    fn render_wheel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let body: Vec<AnyElement> = match &self.wheel {
            Load::Loading => (0..8)
                .map(|ix| {
                    div()
                        
                        .w_40()
                        .h_48()
                        .flex_shrink_0()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().muted)
                        .child(format!("загрузка {ix}"))
                        .into_any_element()
                })
                .collect(),
            Load::Failed(msg) => vec![StatusBanner::error(msg.clone()).into_any_element()],
            Load::Idle => {
                vec![empty_hint("Здесь появится твоё персональное колесо", cx).into_any_element()]
            }
            Load::Ready(items) => items
                .iter()
                .enumerate()
                .map(|(ix, item)| self.render_card(ix, item, cx).into_any_element())
                .collect(),
        };
        v_flex()
            
            .gap_2()
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .child("Тебе понравится"),
            )
            .child(
                h_flex()
                    .id("wheel")
                    
                    .gap_3()
                    .pb_2()
                    .overflow_x_scrollbar()
                    .children(body),
            )
    }

    fn render_card(&self, _ix: usize, item: &WheelItemDto, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let key = format!("{}:{}", item.kind, item.id);
        let data = item.clone();
        div()
            .id(format!("wheel-card-{}", key))
            .w_40()
            .flex_shrink_0()
            .cursor_pointer()
            .gap_1()
            .child(Cover::new(
                item.cover_url.clone(),
                176.0,
                format!("wheelc-{}", item.id.clone()),
            ))
            .child(
                div()
                    .truncate()
                    .text_sm()
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child(item.name.clone()),
            )
            .child(
                div()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(item.description.clone().unwrap_or_default()),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open_item(&data, cx)))
    }
}
