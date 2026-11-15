//! Текучая лента-волна — «как в Яндекс Музыке»: не полоски эквалайзера, а
//! живой заполненный синусоидальный поток, чья амплитуда дышит от уровня
//! музыки. Два слоя бегут с разной скоростью; когда трек на паузе, лента
//! остаётся едва заметной рябью — экран не «умирает».

use std::sync::Arc;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{
    px, App, Bounds, Element, ElementId, GlobalElementId, Hsla, InspectorElementId, IntoElement,
    LayoutId, PathBuilder, Pixels, Style, Window, point, relative,
};

use crate::player::tap::LevelState;

/// Значение полос уровня → амплитуда ленты. Элемент-описание, состояние
/// живёт в атомиках аудио-цепи.
pub struct FlowRibbon {
    levels: Option<Arc<LevelState>>,
    playing: bool,
    fill: Option<Hsla>,
    highlight: bool,
    amp: f32,
    speed: f32,
    id: ElementId,
}

impl FlowRibbon {
    /// hero-волна под обложкой: крупная, двухслойная.
    pub fn hero(levels: Arc<LevelState>, playing: bool) -> Self {
        Self {
            levels: Some(levels),
            playing,
            fill: None,
            highlight: true,
            amp: 16.0,
            speed: 0.85,
            id: ElementId::Name("flow-ribbon-hero".into()),
        }
    }

    /// мини-лента в плеер-баре.
    pub fn mini(levels: Option<Arc<LevelState>>, playing: bool) -> Self {
        Self {
            levels,
            playing,
            fill: None,
            highlight: false,
            amp: 5.0,
            speed: 1.6,
            id: ElementId::Name("flow-ribbon-mini".into()),
        }
    }

    pub fn fill_color(mut self, color: Hsla) -> Self {
        self.fill = Some(color);
        self
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }
}

impl IntoElement for FlowRibbon {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FlowRibbon {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();

        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        ()
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // f32 на абсолютных секундах эпохи теряет суб-секундную точность
        // (квант ~128 с) — фазу снимаем по модулю и только потом в f32
        let secs = std::time::UNIX_EPOCH
            .elapsed()
            .map(|d| d.as_secs_f64())
            .unwrap_or_default();
        let t = ((secs % 1000.0) as f32) * self.speed;

        let (mut a_bass, mut a_mid, mut a_high) = (0f32, 0f32, 0f32);
        if let Some(levels) = self.levels.as_ref() {
            if self.playing {
                a_bass = levels.band(0);
                a_mid = levels.band(2);
                a_high = levels.band(4);
            }
        }
        // амплитуды слоёв: бас — большая медленная волна, середина и верх — рябь
        let amp = self.amp;
        let layers = if self.highlight { 2 } else { 1 };
        let base_fill = self
            .fill
            .unwrap_or_else(|| cx.theme().primary);

        let w = f32::from(bounds.size.width);
        let h = f32::from(bounds.size.height);
        let mid = h * 0.52;
        let ox = f32::from(bounds.origin.x);
        let oy = f32::from(bounds.origin.y);
        let steps = 96usize;

        for layer in 0..layers {
            let (s1, s2, s3, ph) = match layer {
                0 => (0.011, 0.026, 0.052, 0.0),
                _ => (0.016, 0.038, 0.071, 2.4),
            };
            let amp1 = amp * (0.35 + 0.65 * a_bass);
            let amp2 = amp * (0.12 + 0.35 * a_mid);
            let amp3 = amp * (0.05 + 0.22 * a_high);

            let mut pb = PathBuilder::fill();
            for i in 0..=steps {
                let x = w * (i as f32 / steps as f32);
                let y = mid
                    + (x * s1 + t + ph).sin() * amp1
                    + (x * s2 - t * 1.7 + ph).sin() * amp2
                    + (x * s3 + t * 2.6).sin() * amp3;
                let p = point(px(ox + x), px(oy + y));
                if i == 0 {
                    pb.move_to(p);
                } else {
                    pb.line_to(p);
                }
            }
            pb.line_to(point(px(ox + w), px(oy + h)));
            pb.line_to(point(px(ox), px(oy + h)));
            pb.close();
            if let Ok(path) = pb.build() {
                let color = if layer == 0 {
                    base_fill
                } else {
                    Hsla {
                        h: base_fill.h,
                        s: base_fill.s * 0.6,
                        l: (base_fill.l + 0.42).min(0.97),
                        a: 1.0,
                    }
                };
                let opacity = if layer == 0 { 0.92 } else { 0.28 };
                window.paint_path(path, color.opacity(opacity));
            }
        }

        window.request_animation_frame();
    }
}

