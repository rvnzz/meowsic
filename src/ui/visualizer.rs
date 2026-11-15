use std::time::Duration;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{
    div, px, App, Context, Entity, Hsla, IntoElement, ParentElement, Render, SharedString, Styled,
    Task, Window,
};
use gpui_kit::component::h_flex;

use crate::player::Player;

/// Живые полоски уровня: читают RMS-полосы из аудиопотока (player::tap) и
/// перерисовываются сами по себе ~16 fps, не трогая остальной UI.
pub struct Visualizer {
    player: Entity<Player>,
    bands: usize,
    height: f32,
    color: Option<Hsla>,
    shown: Vec<f32>,
    _tick: Task<()>,
}

impl Visualizer {
    pub fn new(
        player: Entity<Player>,
        bands: usize,
        height: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            player,
            bands,
            height,
            color: None,
            shown: vec![0.0; bands],
            _tick: Task::ready(()),
        };
        let weak = cx.entity().downgrade();
        this._tick = cx.spawn(async move |_, cx| loop {
            cx.background_executor().timer(Duration::from_millis(60)).await;
            if weak.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        });
        this
    }

    pub fn with_color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl Render for Visualizer {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let color = self
            .color
            .unwrap_or_else(|| cx.theme().muted_foreground);
        let playing = self.player.read(cx).playing;
        let levels = self.player.read(cx).level_state();
        let bands = self.bands.min(crate::player::tap::BANDS);
        for b in 0..bands {
            let raw = if playing { levels.band(b) } else { 0.0 };
            // лёгкое сглаживание: вверх быстро, вниз мягко
            let cur = self.shown[b];
            self.shown[b] = if raw > cur {
                raw
            } else {
                cur * 0.75 + raw * 0.25
            };
        }
        h_flex()
            .id(SharedString::from(format!("viz-{bands}")))
            .w_full()
            .h_full()
            .gap_1()
            .items_end()
            .children((0..bands).map(|b| {
                let v = self.shown[b].clamp(0.0, 1.0).sqrt();
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(3.0 + v * (self.height - 3.0)))
                    .rounded_full()
                    .bg(if v > 0.75 { color.opacity(1.0) } else { color.opacity(0.75) })
            }))
    }
}
