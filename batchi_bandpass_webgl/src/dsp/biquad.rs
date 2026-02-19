//! Simple biquad filter (Direct Form I) for real-time audio processing.
//!
//! This is dependency-free and `no_std`-friendly (except for `f32` math).
//! Use this to apply bandpass filtering prior to STFT/spectrogram rendering and/or playback.
//!
//! References:
//! - RBJ Audio EQ Cookbook (biquad coefficient formulas)

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    // Coefficients
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // State
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl Biquad {
    #[inline]
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Process a single sample.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        // Transposed Direct Form II (numerically stable)
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Process an entire buffer in-place.
    pub fn process_in_place(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.process(*s);
        }
    }

    /// Design a bandpass biquad (constant skirt gain, peak gain = Q).
    ///
    /// - `sample_rate_hz`: sample rate, e.g. 192_000.0
    /// - `center_hz`: band center frequency, e.g. 45_000.0
    /// - `q`: quality factor, e.g. 8.0 (higher = narrower)
    pub fn bandpass(sample_rate_hz: f32, center_hz: f32, q: f32) -> Self {
        let sr = sample_rate_hz.max(1.0);
        let f0 = center_hz.clamp(1.0, 0.49 * sr);
        let q = q.max(0.001);

        let w0 = 2.0 * core::f32::consts::PI * (f0 / sr);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        // RBJ cookbook bandpass (constant skirt gain)
        let b0 = alpha;
        let b1 = 0.0;
        let b2 = -alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        // normalize
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Design a lowpass biquad (Butterworth-ish, RBJ cookbook).
    pub fn lowpass(sample_rate_hz: f32, cutoff_hz: f32, q: f32) -> Self {
        let sr = sample_rate_hz.max(1.0);
        let fc = cutoff_hz.clamp(1.0, 0.49 * sr);
        let q = q.max(0.001);

        let w0 = 2.0 * core::f32::consts::PI * (fc / sr);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) * 0.5;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Design a highpass biquad (RBJ cookbook).
    pub fn highpass(sample_rate_hz: f32, cutoff_hz: f32, q: f32) -> Self {
        let sr = sample_rate_hz.max(1.0);
        let fc = cutoff_hz.clamp(1.0, 0.49 * sr);
        let q = q.max(0.001);

        let w0 = 2.0 * core::f32::consts::PI * (fc / sr);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 + cos_w0) * 0.5;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}
