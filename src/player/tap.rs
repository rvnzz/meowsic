//! Счётчик уровня по полосам частот: встраивается в цепь rodio после
//! эквалайзера и пишет 6 значений RMS (с пик-детектором) в общие атомики.
//! Никакого FFT: каскады Баттерворта 2-го порядка на полосу достаточно
//! дёшевы для аудио-потока и дают живые «полоски».

use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};

use rodio::{ChannelCount, SampleRate, Source as RodioSource};

use super::eq::Biquad;

pub const BANDS: usize = 6;
/// Границы полос: 120/400/900/1800/3500 Гц; последняя — всё выше 3500.
const CUTOFFS: [f32; BANDS - 1] = [120.0, 400.0, 900.0, 1800.0, 3500.0];
/// Верхние полосы тише по энергии — компенсируем чувствительность.
const GAIN: [f32; BANDS] = [1.0, 1.6, 2.4, 3.4, 4.6, 6.0];
const BLOCK: usize = 128;

/// Общее состояние уровней и бит-детектора для DSP-потока и UI.
pub struct LevelState {
    bands: [AtomicU32; BANDS],
    beat_seq: AtomicU64,
    beat_strength: AtomicU32,
    beat_at_ms: AtomicU64,
}

impl LevelState {
    pub(crate) fn new() -> Self {
        Self {
            bands: std::array::from_fn(|_| AtomicU32::new(0)),
            beat_seq: AtomicU64::new(0),
            beat_strength: AtomicU32::new(0),
            beat_at_ms: AtomicU64::new(0),
        }
    }

    pub fn band(&self, ix: usize) -> f32 {
        f32::from_bits(self.bands[ix].load(Ordering::Relaxed))
    }

    pub fn zero(&self) {
        for b in &self.bands {
            b.store(0, Ordering::Relaxed);
        }
        self.beat_strength.store(0, Ordering::Relaxed);
    }

    /// (номер бита, сила 0..1, unix-мс удара) — UI сам строит огибающую.
    pub fn beat(&self) -> (u64, f32, u64) {
        (
            self.beat_seq.load(Ordering::Relaxed),
            f32::from_bits(self.beat_strength.load(Ordering::Relaxed)),
            self.beat_at_ms.load(Ordering::Relaxed),
        )
    }
}

/// Фильтр-обёртка: считает энергию полос и передаёт сэмплы дальше без изменений.
pub struct LevelTap<I>
where
    I: RodioSource<Item = f32>,
{
    inner: I,
    state: Arc<LevelState>,
    ch: usize,
    chix: usize,
    frames: usize,
    rate: u32,
    /// [канал][фильтр]: 0..4 — lowpass по границам, 5..9 — highpass
    filters: Vec<Vec<Biquad>>,
    sum: [f64; BANDS],
    bass_slow: f32,
    blocks_since_beat: usize,
}

impl<I: RodioSource<Item = f32>> LevelTap<I> {
    pub(crate) fn new(inner: I, state: Arc<LevelState>) -> Self {
        let ch = inner.channels().get() as usize;
        let rate = inner.sample_rate().get();
        Self {
            inner,
            state,
            ch,
            chix: 0,
            frames: 0,
            rate,
            filters: Vec::new(),
            sum: [0.0; BANDS],
            bass_slow: 0.0,
            blocks_since_beat: usize::MAX / 2,
        }
    }

    fn ensure_filters(&mut self) {
        if !self.filters.is_empty() {
            return;
        }
        let mut specs: Vec<Biquad> = Vec::with_capacity(2 * (BANDS - 1));
        for f in CUTOFFS {
            specs.push(Biquad::lowpass(f, self.rate));
        }
        for f in CUTOFFS {
            specs.push(Biquad::highpass(f, self.rate));
        }
        self.filters = (0..self.ch.max(1))
            .map(|_| {
                specs
                    .iter()
                    .map(|t| {
                        let mut b = *t;
                        b.reset();
                        b
                    })
                    .collect()
            })
            .collect();
    }

    pub(crate) fn push_block(&mut self) {
        let n = self.frames.max(1) as f64;
        for b in 0..BANDS {
            let rms = (self.sum[b] / n).sqrt() as f32;
            let prev = self.state.band(b);
            // пик-детектор с плавным спадом: полоски «дышат», а не скачут
            let v = (rms * GAIN[b]).clamp(0.0, 1.0).max(prev * 0.88);
            self.state.bands[b].store(v.to_bits(), Ordering::Relaxed);
        }
        self.detect_beat();
        self.sum = [0.0; BANDS];
        self.frames = 0;
    }

    /// Он-сет по басовой энергии: всплеск относительно медленного среднего.
    fn detect_beat(&mut self) {
        let bass = (self.sum[0] / BLOCK.max(1) as f64).sqrt() as f32;
        // окно ~1.5 с на 128-кадровых блоках (~2.9 мс)
        self.bass_slow = self.bass_slow * 0.995 + bass * 0.005;
        self.blocks_since_beat += 1;
        let min_gap = 40; // ~120 мс между ударами
        if bass > self.bass_slow * 1.5 + 0.015 && self.blocks_since_beat >= min_gap {
            let strength = ((bass - self.bass_slow) * 6.0).clamp(0.25, 1.0);
            self.bass_slow = self.bass_slow.max(bass * 0.55); // не даём серии ложных
            self.blocks_since_beat = 0;
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or_default();
            self.state.beat_at_ms.store(now_ms, Ordering::Relaxed);
            self.state.beat_strength.store(strength.to_bits(), Ordering::Relaxed);
            self.state.beat_seq.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl<I: RodioSource<Item = f32>> Iterator for LevelTap<I> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let x = self.inner.next()?;
        if self.filters.is_empty() {
            self.ensure_filters();
        }
        let ch = self.ch.max(1);
        let c = (self.chix % ch).min(self.filters.len().saturating_sub(1));
        let filters = &mut self.filters[c];
        let sum = &mut self.sum;
        // энергия полос из независимых LP/HP вокруг границ
        let mut lp = [0f64; BANDS - 1];
        let mut hp = [0f64; BANDS - 1];
        for k in 0..BANDS - 1 {
            let y = filters[k].process(x);
            lp[k] = (y * y) as f64;
            let y = filters[(BANDS - 1) + k].process(x);
            hp[k] = (y * y) as f64;
        }
        sum[0] += lp[0];
        for b in 1..BANDS - 1 {
            sum[b] += (hp[b - 1] + lp[b]) * 0.5;
        }
        sum[BANDS - 1] += hp[BANDS - 2];

        self.chix += 1;
        if self.chix % ch == 0 {
            self.frames += 1;
            if self.frames >= BLOCK {
                self.push_block();
            }
        }
        Some(x)
    }
}

impl<I: RodioSource<Item = f32>> RodioSource for LevelTap<I> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::{NonZeroU16, NonZeroU32};

    /// Синус 100 Гц должен дать максимум в первой полосе и почти ноль в высокой.
    #[test]
    fn bass_sine_lights_low_band() {
        let n = 44100 * 2; // 2 секунды
        let data: Vec<f32> = (0..n * 2)
            .map(|i| {
                let t = (i / 2) as f32 / 44100.0;
                0.8 * (2.0 * std::f32::consts::PI * 100.0 * t).sin()
            })
            .collect();
        let state = Arc::new(LevelState::new());
        let inner = rodio::buffer::SamplesBuffer::new(
            NonZeroU16::new(2).unwrap(),
            NonZeroU32::new(44100).unwrap(),
            data,
        );
        let tap = LevelTap::new(inner, Arc::clone(&state));
        let mut it = tap;
        while it.next().is_some() {}
        // проталкиваем хвост блока
        it.push_block();
        let low = state.band(0);
        let high = state.band(5);
        assert!(low > 0.3, "бас не загорелся: {low}");
        assert!(high < 0.15, "верхняя полоса шумит: {high}");
    }
}
