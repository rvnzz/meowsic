//! Звезда по центру, которая ебашит в бит.
//!
//! Два элемента:
//! * [`StarScene`] — полноэкранная сцена «как в референсе ЯМ»: длинные
//!   лучи-проблески от фокусной точки, большая мягкая и малая острая
//!   четырёхконечные искры с вогнутыми лучами, ореол — всё дышит и
//!   разлетается на бит. Рисуется ПЕРЕД контентом, обложка ложится сверху.
//! * [`BeatStar`] — компактная звезда для плеер-бара.

use std::sync::Arc;

use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    linear_color_stop, linear_gradient, px, App, Bounds, Element, ElementId, GlobalElementId,
    Hsla, InspectorElementId, IntoElement, LayoutId, PathBuilder, Pixels, Style, Window, point,
    relative,
};

use crate::player::tap::LevelState;

fn read_beat(levels: &Option<Arc<LevelState>>, playing: bool, now_ms: u64) -> (f32, f32) {
    let (mut bass, mut env) = (0f32, 0f32);
    if let Some(state) = levels.as_ref() {
        if playing {
            bass = state.band(0);
            let (_seq, strength, at_ms) = state.beat();
            if at_ms > 0 && now_ms > at_ms {
                let dt = (now_ms - at_ms).min(600) as f32;
                env = (strength * (-dt * 0.009).exp()).min(1.0);
            }
        }
    }
    (bass, env)
}

/// Точка фокуса сцены (доли размеров bounds) — под центром обложки.
#[derive(Clone, Copy)]
pub struct FocusPoint {
    pub fx: f32,
    pub fy: f32,
}

pub struct StarScene {
    levels: Arc<LevelState>,
    playing: bool,
    color: Option<Hsla>,
    focus: FocusPoint,
    id: ElementId,
}

impl StarScene {
    pub fn new(levels: Arc<LevelState>, playing: bool, focus: FocusPoint) -> Self {
        Self {
            levels,
            playing,
            color: None,
            focus,
            id: ElementId::Name("beat-star-scene".into()),
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

impl IntoElement for StarScene {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for StarScene {
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
        let base = self.color.unwrap_or_else(|| cx.theme().primary);
        let light = Hsla {
            l: (base.l + 0.42).min(0.98),
            s: base.s * 0.7,
            ..base
        };

        let secs = std::time::UNIX_EPOCH
            .elapsed()
            .map(|d| d.as_secs_f64())
            .unwrap_or_default();
        let t = ((secs % 1000.0) as f32) * 0.16;
        let now_ms = (secs * 1000.0) as u64;
        let (bass, env) = read_beat(&Some(Arc::clone(&self.levels)), self.playing, now_ms);

        let w = f32::from(bounds.size.width);
        let h = f32::from(bounds.size.height);
        let ox = f32::from(bounds.origin.x) + w * self.focus.fx;
        let oy = f32::from(bounds.origin.y) + h * self.focus.fy;
        let diag = (w * w + h * h).sqrt();

        // 1) лучи-проблески: 7 штук, детерминированная «лотерея» длины
        let n_rays = 11usize;
        for i in 0..n_rays {
            let fi = i as f32;
            let seed = (fi * 12.9898).sin() * 43758.5453;
            let jitter = seed - seed.floor(); // 0..1
            let angle = t * 0.6 + fi * std::f32::consts::TAU / n_rays as f32 + jitter * 0.5;
            let len = diag * (0.30 + 0.66 * jitter) * (0.85 + 0.6 * env + 0.15 * bass);
            let half_width = 0.035 + 0.045 * jitter + 0.040 * env;

            let ax = angle.cos();
            let ay = angle.sin();
            let px1 = (angle + half_width).cos();
            let py1 = (angle + half_width).sin();
            let px2 = (angle - half_width).cos();
            let py2 = (angle - half_width).sin();
            let mut pb = PathBuilder::fill();
            pb.move_to(point(px(ox + 12.0), px(oy)));
            pb.line_to(point(px(ox + len * px1), px(oy + len * py1)));
            pb.line_to(point(px(ox + len * ax * 1.04), px(oy + len * ay * 1.04)));
            pb.line_to(point(px(ox + len * px2), px(oy + len * py2)));
            pb.close();
            if let Ok(path) = pb.build() {
                let deg = angle.to_degrees();
                let inner = base.opacity(0.09 + 0.34 * env);
                let outer = base.opacity(0.0);
                window.paint_path(
                    path,
                    linear_gradient(
                        deg + 90.0,
                        linear_color_stop(inner, 0.0),
                        linear_color_stop(outer, 1.0),
                    ),
                );
            }
        }

        // 2) широкая мягкая подложка-звезда (дышит на бите)
        draw_sparkle(
            window,
            ox,
            oy,
            diag * (0.20 + 0.12 * env),
            t * 0.5,
            0.26 + 0.10 * env,
            base.opacity(0.07 + 0.11 * env),
        );
        // 3) диагональный крест — тонкие короткие лучи между основных
        draw_sparkle(
            window,
            ox,
            oy,
            diag * (0.09 + 0.075 * env),
            std::f32::consts::FRAC_PI_4 - t * 0.7,
            0.07 + 0.05 * env,
            light.opacity(0.55 + 0.40 * env),
        );
        // 4) главная искра: длинные тонкие лучи, на бите — вспышка
        draw_sparkle(
            window,
            ox,
            oy,
            diag * (0.23 + 0.12 * env + 0.015 * bass),
            t * 0.5,
            0.055 + 0.045 * env,
            light.opacity(0.95),
        );
        // 5) горячее ядро в центре пересечения лучей
        let core_r = diag * (0.010 + 0.016 * env);
        let mut dot = PathBuilder::fill();
        for i in 0..=24 {
            let a = std::f32::consts::TAU * i as f32 / 24.0;
            let x = ox + core_r * a.cos();
            let y = oy + core_r * a.sin();
            if i == 0 {
                dot.move_to(point(px(x), px(y)));
            } else {
                dot.line_to(point(px(x), px(y)));
            }
        }
        dot.close();
        if let Ok(path) = dot.build() {
            window.paint_path(path, light.opacity(0.85));
        }

        window.request_animation_frame();
    }
}

/// Искра из четырёх лучей на кубических Безье: острые иглы-кончики и
/// глубокая вогнутая «талия» — как фирменная звезда ЯМ. waist — доля
/// радиуса контрольных точек у центра: меньше — тоньше иглы; spread —
/// угловой разлёт контрольных точек вокруг прогиба.
fn draw_sparkle(
    window: &mut Window,
    ox: f32,
    oy: f32,
    r: f32,
    rot: f32,
    waist: f32,
    color: Hsla,
) {
    use std::f32::consts::FRAC_PI_2;
    let tip = |k: i32| {
        let a = rot + k as f32 * FRAC_PI_2;
        point(px(ox + r * a.cos()), px(oy + r * a.sin()))
    };
    // чем тоньше лучи (меньше waist), тем «прямолинейнее» должен быть спуск
    let spread = 0.62 * (1.0 - waist.min(0.9));
    let mut pb = PathBuilder::fill();
    pb.move_to(tip(0));
    for k in 0..4 {
        let mid = rot + (k as f32 + 0.5) * FRAC_PI_2;
        let a1 = mid - spread;
        let a2 = mid + spread;
        let ca = point(px(ox + r * waist * a1.cos()), px(oy + r * waist * a1.sin()));
        let cb = point(px(ox + r * waist * a2.cos()), px(oy + r * waist * a2.sin()));
        pb.cubic_bezier_to(tip(k + 1), ca, cb);
    }
    pb.close();
    if let Ok(path) = pb.build() {
        window.paint_path(path, color);
    }
}

pub struct BeatStar {
    levels: Option<Arc<LevelState>>,
    playing: bool,
    color: Option<Hsla>,
    spin: f32,
    kick: f32,
    id: ElementId,
}

impl BeatStar {
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
        let secs = std::time::UNIX_EPOCH
            .elapsed()
            .map(|d| d.as_secs_f64())
            .unwrap_or_default();
        let t = ((secs % 1000.0) as f32) * self.spin;
        let now_ms = (secs * 1000.0) as u64;
        let (bass, env) = read_beat(&self.levels, self.playing, now_ms);

        let center = bounds.center();
        let ox = f32::from(center.x);
        let oy = f32::from(center.y);
        let r = 0.5 * f32::from(bounds.size.width.min(bounds.size.height))
            * (0.82 + 0.06 * bass)
            * (1.0 + self.kick * env);
        draw_sparkle(window, ox, oy, r, t, 0.06 + 0.06 * env, base_color);
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
