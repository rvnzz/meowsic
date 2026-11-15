//! Звезда по центру, которая ебашит в бит.
//!
//! Форма — четырёхконечная «искра» (как в стикерах ЯМ): r(θ) с острыми
//! лучами; на каждом бите (он-сет-детектор в player::tap) звезда получает
//! удар: резкий выброс масштаба и длины лучей, затем экспоненциальное
//! затухание огибающей. Фоном — мягкое свечение, дышащее от баса.

use std::sync::Arc;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{
    linear_color_stop, linear_gradient, px, App, Bounds, Element, ElementId, GlobalElementId,
    Hsla, InspectorElementId, IntoElement, LayoutId, PathBuilder, Pixels, Style, Window, point,
    relative,
};

use crate::player::tap::LevelState;

pub struct BeatStar {
    levels: Option<Arc<LevelState>>,
    playing: bool,
    color: Option<Hsla>,
    spin: f32,
    kick: f32,
    id: ElementId,
}

impl BeatStar {
    /// Большая звезда на странице волны.
    pub fn hero(levels: Arc<LevelState>, playing: bool) -> Self {
        Self {
            levels: Some(levels),
            playing,
            color: None,
            spin: 0.22,
            kick: 0.6,
            id: ElementId::Name("beat-star-hero".into()),
        }
    }

    /// Мини-звезда в плеер-баре.
    pub fn mini(levels: Option<Arc<LevelState>>, playing: bool) -> Self {
        Self {
            levels,
            playing,
            color: None,
            spin: 0.5,
            kick: 0.45,
            id: ElementId::Name("beat-star-mini".into()),
        }
    }

    pub fn fill_color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }
}

impl IntoElement for BeatStar {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BeatStar {
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
        let base_color = self.color.unwrap_or_else(|| cx.theme().primary);

        // время: f64 → по модулю → f32 (в f32 эпоха теряет дробную часть)
        let secs = std::time::UNIX_EPOCH
            .elapsed()
            .map(|d| d.as_secs_f64())
            .unwrap_or_default();
        let t = ((secs % 1000.0) as f32) * self.spin;
        let now_ms = (secs * 1000.0) as u64;

        let mut bass = 0f32;
        let mut env = 0f32; // огибающая удара 0..1
        if let Some(state) = self.levels.as_ref() {
            if self.playing {
                bass = state.band(0);
                let (_seq, strength, at_ms) = state.beat();
                if at_ms > 0 && now_ms > at_ms {
                    let dt = (now_ms - at_ms).min(500) as f32;
                    env = strength * (-dt * 0.010).exp(); // τ ≈ 100 мс
                }
            }
        }

        let w = f32::from(bounds.size.width);
        let h = f32::from(bounds.size.height);
        let center = bounds.center();
        let ox = f32::from(center.x);
        let oy = f32::from(center.y);

        let base_r = 0.5 * w.min(h) * 0.86;
        let scale = (0.94 + 0.10 * bass) * (1.0 + self.kick * env);
        let r_main = base_r * scale;
        // в покое — острые тонкие лучи; на ударе распухают и удлиняются
        let spike_pow = 3.0 - 1.3 * env;
        let core = 0.13 + 0.22 * env;
        let arms = 2.0f32; // |cos(2θ)| даёт 4 луча

        // 1) мягкое свечение позади
        let glow_r = r_main * (1.25 + 0.5 * env);
        let mut glow = PathBuilder::fill();
        let gsteps = 48;
        for i in 0..=gsteps {
            let a = std::f32::consts::TAU * (i as f32 / gsteps as f32);
            let wob = 1.0 + 0.06 * (a * 3.0 + t * 2.0).sin();
            let x = ox + glow_r * wob * a.cos();
            let y = oy + glow_r * wob * a.sin();
            if i == 0 {
                glow.move_to(point(px(x), px(y)));
            } else {
                glow.line_to(point(px(x), px(y)));
            }
        }
        glow.close();
        if let Ok(path) = glow.build() {
            window.paint_path(path, base_color.opacity(0.10 + 0.30 * env + 0.06 * bass));
        }

        // 2) сама звезда
        let steps = 160;
        let mut star = PathBuilder::fill();
        for i in 0..=steps {
            let a = std::f32::consts::TAU * (i as f32 / steps as f32);
            let spike = (arms * (a + t)).cos().abs().powf(spike_pow);
            let wobble = 1.0 + 0.02 * (4.0 * a + t).sin();
            let r = r_main * (core + (1.0 - core) * spike) * wobble;
            let x = ox + r * a.cos();
            let y = oy + r * a.sin();
            if i == 0 {
                star.move_to(point(px(x), px(y)));
            } else {
                star.line_to(point(px(x), px(y)));
            }
        }
        star.close();
        if let Ok(path) = star.build() {
            let light = Hsla {
                h: base_color.h,
                s: base_color.s * 0.75,
                l: (base_color.l + 0.45).min(0.98),
                a: 1.0,
            };
            // на пике удара — засветленная верхняя половина градиента
            let top = if env > 0.4 { light } else { base_color };
            window.paint_path(
                path,
                linear_gradient(
                    120.0,
                    linear_color_stop(top, 0.0),
                    linear_color_stop(base_color.opacity(0.9), 1.0),
                ),
            );
        }

        window.request_animation_frame();
    }
}

#[cfg(test)]
mod beat_tests {
    use super::*;
    use crate::player::tap::LevelTap;
    use std::num::{NonZeroU16, NonZeroU32};

    /// Посылка баса после затишья → как минимум один «удар».
    #[test]
    fn onset_pulse_is_detected() {
        let rate = 44100usize;
        let mut data: Vec<f32> = Vec::new();
        // 1.5 с тишины-шума, затем жирный басовый удар
        for _ in 0..rate * 3 / 2 {
            data.push(0.01);
            data.push(0.01);
        }
        for i in 0..rate / 2 {
            let t = i as f32 / rate as f32;
            let s = 0.95 * (2.0 * std::f32::consts::PI * 70.0 * t).sin();
            data.push(s);
            data.push(s);
        }
        let state = Arc::new(LevelState::new());
        let inner = rodio::buffer::SamplesBuffer::new(
            NonZeroU16::new(2).unwrap(),
            NonZeroU32::new(rate as u32).unwrap(),
            data,
        );
        let mut tap = LevelTap::new(inner, Arc::clone(&state));
        while tap.next().is_some() {}
        tap.push_block();
        let (seq, strength, at_ms) = state.beat();
        assert!(seq >= 1, "удар не пойман: seq={seq}");
        assert!(strength > 0.2 && at_ms > 0, "сила/время не записаны");
    }
}
