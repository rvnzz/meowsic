//! Многополосный эквалайзер: цепочка пиковых биквадов RBJ прямо на потоке
//! сэмплов между декодером и выходами rodio.

use rodio::{ChannelCount, SampleRate, Source};

const Q: f32 = 1.1;

#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    /// RBJ peaking EQ по cookbook.
    fn peak(freq: f32, sample_rate: u32, gain_db: f32) -> Self {
        let a = (10f64).powf(gain_db as f64 / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * freq as f64 / sample_rate as f64;
        let sin = w0.sin();
        let cos = w0.cos();
        let alpha = sin / (2.0 * Q as f64);

        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let xf = x as f64;
        let y = self.b0 * xf + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = xf;
        self.y2 = self.y1;
        self.y1 = y;
        y.clamp(-4.0, 4.0) as f32
    }
}

/// Прогоняет каждый канал через все активные полосы. Полосы с нулевым
/// усилением пропускаются, чтобы не жечь точность впустую.
pub struct EqSource<I>
where
    I: Source<Item = f32>,
{
    inner: I,
    gains: Vec<(f32, f32)>, // (частота, dB) только ненулевые
    states: Vec<Vec<Biquad>>, // [канал][полоса]
    ch: usize,
    ix: usize,
    rate: u32,
}

impl<I: Source<Item = f32>> EqSource<I> {
    pub fn new(inner: I, frequencies: &[f32], gains_db: &[f32]) -> Self {
        let mut gains = Vec::new();
        for (ix, g) in gains_db.iter().enumerate() {
            if g.abs() > 0.01 {
                let f = frequencies.get(ix).copied().unwrap_or(1000.0);
                gains.push((f, *g));
            }
        }
        let ch = inner.channels().get() as usize;
        Self {
            inner,
            gains,
            states: Vec::new(),
            ch,
            ix: 0,
            rate: 44100,
        }
    }

    fn ensure_bands(&mut self) {
        if !self.states.is_empty() {
            return;
        }
        self.rate = self.inner.sample_rate().get();
        self.states = (0..self.ch)
            .map(|_| {
                self.gains
                    .iter()
                    .map(|(f, g)| Biquad::peak(*f, self.rate, *g))
                    .collect()
            })
            .collect();
    }
}

impl<I: Source<Item = f32>> Iterator for EqSource<I> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let x = self.inner.next()?;
        if self.gains.is_empty() {
            return Some(x);
        }
        self.ensure_bands();
        let ch = self.ch.max(1);
        let band = self.ix % ch;
        self.ix += 1;
        let mut y = x;
        let last = self.states.len().saturating_sub(1);
        for state in &mut self.states[band.min(last)] {
            y = state.process(y);
        }
        Some(y)
    }
}

impl<I: Source<Item = f32>> Source for EqSource<I> {
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

    #[test]
    fn flat_gain_does_not_change_dc() {
        // постоянный сигнал через 0dB цепочку не должен плыть
        let bq = Biquad::peak(1000.0, 44100, 0.0);
        let mut bq = {
            let mut copy = bq;
            copy.b1 = bq.b1;
            copy
        };
        let mut last = 0.0f32;
        for _ in 0..1000 {
            last = bq.process(0.5);
        }
        assert!((last - 0.5).abs() < 1e-3, "получено {last}");
    }

    #[test]
    fn boost_amplifies_sine_at_band() {
        let fs = 44100u32;
        let mut bq = Biquad::peak(1000.0, fs, 6.0);
        // прогреваем
        let phase_step = 2.0 * std::f64::consts::PI * 1000.0 / fs as f64;
        let mut prev = 0.0f32;
        let mut peak = 0.0f32;
        for n in 0..4000 {
            let x = ((phase_step * n as f64).sin()) as f32;
            let y = bq.process(x);
            if n > 2000 {
                peak = peak.max(y.abs());
            }
            prev = y;
        }
        assert!(prev.is_finite());
        assert!(peak > 1.4 && peak < 3.5, "усиление {peak}");
    }
}
