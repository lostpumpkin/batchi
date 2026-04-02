# Integrate bandpass filtering + WebGL 3D waterfall into Batchi (docs-upgrade-echoscope branch)

This bundle adds two things:

1) **Bandpass filtering**
   - Time-domain IIR filter chain (`highpass -> lowpass -> optional narrow bandpass`)
   - Intended to run:
     - before STFT / spectrogram computation (so visuals clean up)
     - optionally in playback chain (so you can *listen* to just a band)

2) **WebGL2 3D waterfall renderer**
   - A true 3D surface (time × freq × energy) like the fancy reels
   - Uses a float texture (R32F) and a generated mesh grid.

> You already have a Canvas2D spectrogram view.
> Add a new view mode in the UI, and render via WebGL when selected.

---

## 1) Bandpass filtering

### Files added
- `src/dsp/biquad.rs`
- `src/dsp/bandpass.rs`

### Wire into your module tree
Add `src/dsp/mod.rs` (or edit your existing) to export:

```rust
pub mod biquad;
pub mod bandpass;
```

### Where to apply it

You likely have one (or both) of these paths:

**A) Analysis path (recommended first):**
- Wherever you build the spectrogram frames from PCM samples:
  - create a `BandpassChain`
  - run `process_in_place(&mut frame_samples)` before FFT

This gives you:
- less broadband noise
- cleaner ridge extraction for SyllableTrace
- cleaner WebGL surface

**B) Playback path (optional):**
- If you have a playback pipeline node, apply the same chain there.
- Gate it behind a UI toggle "Bandpass affects playback".

### State/UI params
Add to AppState (names are suggestions):

- `bandpass_enabled: bool`
- `bandpass_low_hz: f32`
- `bandpass_high_hz: f32`
- `bandpass_q: f32`
- `bandpass_center_hz: f32` (0 = off)
- `bandpass_center_q: f32`

Default for bats:
- enabled: false (user can toggle)
- low: 15_000
- high: 120_000
- q: 0.707
- center_hz: 0
- center_q: 8

### Spectrogram-domain masking (cheap and often enough)
Even without time-domain filtering, you can mask the spectrogram bins outside [low, high] before rendering:
- Any bin outside band: set db = floor_db (e.g. -120)

This is great for WebGL and for the ridge trace.

---

## 2) WebGL 3D waterfall

### Files added
- `src/webgl/waterfall_renderer.rs`
- `src/components/spectrogram_webgl.rs`

### Module exports
Add `src/webgl/mod.rs`:

```rust
pub mod waterfall_renderer;
```

and export `components::spectrogram_webgl`.

### Add a View Mode
Add a UI select:
- Spectrogram (2D)
- SyllableTrace (2D ribbon)
- Waterfall 3D (WebGL)

Then in your main spectrogram component:
- if view_mode == Waterfall3D:
  - render `<SpectrogramWebGl3D ... />`
- else
  - render your existing Canvas2D view(s)

### Feeding data
The WebGL renderer expects:
- `t_bins`, `f_bins`
- `db: Vec<f32>` length `t_bins*f_bins`, time-major

If your `SpectrogramData` differs, adapt in a conversion step:
- downsample time to <= ~1000
- downsample freq to <= ~512
- keep values in dB (or normalized with a floor)

### WebGL float texture notes
Most desktop browsers support sampling `R32F` in WebGL2.
If you hit a device that fails:
- fall back to `R16F` (half float) or `RGBA16F`
- or pack to `RGBA8` with manual unpack in shader

---

## Recommended sequencing
1) Add **spectrogram-domain masking** (instant UX win)
2) Add **bandpass time-domain** for analysis frames
3) Add WebGL 3D view mode (surface wow)
4) Add playback bandpass toggle (optional)

If you want, I can generate an exact patch list once you paste:
- the file that constructs spectrogram frames (the function name + path)
- the state file that holds spectrogram settings
- the component that renders the spectrogram canvas
