//! Bandpass filtering utilities for Batchi/EchoScope.
//!
//! There are two complementary ways to "bandpass":
//! 1) **Time-domain filtering** (IIR biquads) — affects playback and analysis.
//! 2) **Spectrogram-domain masking** — cheap and perfect for visualization; does NOT affect playback.
//!
//! This module provides (1). For (2), mask bins outside the selected band
//! when building the image/texture.

use crate::dsp::biquad::Biquad;

#[derive(Clone, Copy, Debug)]
pub struct BandpassParams {
    /// Enable/disable filtering.
    pub enabled: bool,
    /// Low cutoff Hz (for a biquad HPF stage).
    pub low_hz: f32,
    /// High cutoff Hz (for a biquad LPF stage).
    pub high_hz: f32,
    /// Q for the HPF/LPF stages (0.707 ~ Butterworth-ish).
    pub q: f32,
    /// Optional extra narrow bandpass (center/Q). Set center_hz <= 0 to disable.
    pub center_hz: f32,
    pub center_q: f32,
}

impl Default for BandpassParams {
    fn default() -> Self {
        Self {
            enabled: false,
            low_hz: 15_000.0,
            high_hz: 120_000.0,
            q: 0.707,
            center_hz: 0.0,
            center_q: 8.0,
        }
    }
}

/// A small filter chain: highpass -> lowpass -> (optional) bandpass.
/// Designed to be fast enough for real-time use.
#[derive(Clone, Debug)]
pub struct BandpassChain {
    sr: f32,
    hpf: Biquad,
    lpf: Biquad,
    bpf: Option<Biquad>,
    params: BandpassParams,
}

impl BandpassChain {
    pub fn new(sample_rate_hz: f32, params: BandpassParams) -> Self {
        let mut c = Self {
            sr: sample_rate_hz.max(1.0),
            hpf: Biquad::default(),
            lpf: Biquad::default(),
            bpf: None,
            params,
        };
        c.rebuild();
        c
    }

    pub fn set_params(&mut self, params: BandpassParams) {
        self.params = params;
        self.rebuild();
    }

    pub fn params(&self) -> BandpassParams {
        self.params
    }

    pub fn reset(&mut self) {
        self.hpf.reset();
        self.lpf.reset();
        if let Some(b) = &mut self.bpf { b.reset(); }
    }

    fn rebuild(&mut self) {
        // clamp
        let sr = self.sr;
        let low = self.params.low_hz.clamp(1.0, 0.49 * sr);
        let high = self.params.high_hz.clamp(1.0, 0.49 * sr);
        let q = self.params.q.max(0.001);

        // ensure order
        let (low, high) = if low <= high { (low, high) } else { (high, low) };

        self.hpf = Biquad::highpass(sr, low, q);
        self.lpf = Biquad::lowpass(sr, high, q);

        if self.params.center_hz > 0.0 {
            self.bpf = Some(Biquad::bandpass(sr, self.params.center_hz, self.params.center_q.max(0.001)));
        } else {
            self.bpf = None;
        }
    }

    /// Process a single sample through the chain.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.params.enabled { return x; }
        let mut y = self.hpf.process(x);
        y = self.lpf.process(y);
        if let Some(b) = &mut self.bpf {
            y = b.process(y);
        }
        y
    }

    /// Process buffer in-place.
    pub fn process_in_place(&mut self, buf: &mut [f32]) {
        if !self.params.enabled { return; }
        for s in buf.iter_mut() {
            let mut y = self.hpf.process(*s);
            y = self.lpf.process(y);
            if let Some(b) = &mut self.bpf { y = b.process(y); }
            *s = y;
        }
    }
}
