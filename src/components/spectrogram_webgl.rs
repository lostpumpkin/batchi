//! Leptos component wrapper for the WebGL 3D waterfall spectrogram.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext};
use std::rc::Rc;

use crate::webgl::waterfall_renderer::Waterfall3D;
use crate::types::SpectrogramData;
use crate::dsp::bandpass::BandpassParams;
use crate::state::BandpassMode;

// Unsafe wrapper to allow storing !Send types in signals (WASM is single-threaded)
#[derive(Clone)]
struct SendWrapper<T>(T);
unsafe impl<T> Send for SendWrapper<T> {}
unsafe impl<T> Sync for SendWrapper<T> {}

#[component]
pub fn SpectrogramWebGl3D(
    data: Signal<Option<SpectrogramData>>,
    zgain: Signal<f32>,
    floor_db: Signal<f32>,
    contrast: Signal<f32>,
    bandpass_params: Signal<BandpassParams>,
    bandpass_mode: Signal<BandpassMode>,
) -> impl IntoView {
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();

    // renderer state: StoredValue because WebGL context is !Send
    let renderer = store_value(None::<SendWrapper<Waterfall3D>>);
    let renderer_ready = create_rw_signal(false);
    let last_dims = create_rw_signal::<(i32,i32)>( (0,0) );

    // init on mount
    create_effect(move |_| {
        let Some(canvas) = canvas_ref.get() else { return; };
        let gl: WebGl2RenderingContext = canvas
            .get_context("webgl2").ok().flatten()
            .and_then(|c| c.dyn_into::<WebGl2RenderingContext>().ok())
            .expect("WebGL2 unavailable");

        let wf = Waterfall3D::new(gl).expect("failed to init Waterfall3D");
        renderer.set_value(Some(SendWrapper(wf)));
        renderer_ready.set(true);

        // Make canvas size follow CSS pixels with DPR
        let window = web_sys::window().unwrap();
        let dpr = window.device_pixel_ratio().min(2.0).max(1.0);

        let rect = canvas.get_bounding_client_rect();
        let w = (rect.width() * dpr).floor() as i32;
        let h = (rect.height() * dpr).floor() as i32;
        canvas.set_width(w as u32);
        canvas.set_height(h as u32);
        last_dims.set((w,h));
    });

    // upload when data changes
    create_effect(move |_| {
        if !renderer_ready.get() { return; }
        let Some(d) = data.get() else { return; };

        // Convert to dB grid and flatten
        let t_bins = d.columns.len();
        if t_bins == 0 { return; }
        let f_bins = d.columns[0].magnitudes.len();

        let mut db_data = vec![-120.0f32; t_bins * f_bins];

        // Find max magnitude for normalization
        let max_mag = d.columns.iter()
            .flat_map(|c| c.magnitudes.iter())
            .copied()
            .fold(0.0f32, f32::max)
            .max(1e-9);

        let bp = bandpass_params.get();
        let mode = bandpass_mode.get();
        let mask_active = bp.enabled && mode == BandpassMode::Visualization;

        for (t, col) in d.columns.iter().enumerate() {
            for (f, &mag) in col.magnitudes.iter().enumerate() {
                let freq_hz = f as f32 * d.freq_resolution as f32;

                let mut val_db = if mag > 0.0 {
                    20.0 * (mag / max_mag).log10()
                } else {
                    -120.0
                };

                // Apply mask
                if mask_active {
                    if freq_hz < bp.low_hz || freq_hz > bp.high_hz {
                        val_db = -120.0;
                    }
                }

                // Write to texture buffer
                // Texture expects: x=time, y=freq
                let idx = f * t_bins + t;
                if idx < db_data.len() {
                    db_data[idx] = val_db;
                }
            }
        }

        renderer.update_value(|wrapper| {
            if let Some(w) = wrapper {
                let _ = w.0.upload_db_texture(t_bins as i32, f_bins as i32, &db_data);
            }
        });
    });

    // animation loop
    create_effect(move |_| {
        let Some(canvas) = canvas_ref.get() else { return; };
        let window = web_sys::window().unwrap();
        let window_loop = window.clone();

        let f = Rc::new(std::cell::RefCell::new(None::<Closure<dyn FnMut()>>));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
            // Resize to match layout (cheap)
            let dpr = window_loop.device_pixel_ratio().min(2.0).max(1.0);
            let rect = canvas.get_bounding_client_rect();
            let w = (rect.width() * dpr).floor() as i32;
            let h = (rect.height() * dpr).floor() as i32;
            let (lw, lh) = last_dims.get_untracked();
            if w != lw || h != lh {
                canvas.set_width(w as u32);
                canvas.set_height(h as u32);
                last_dims.set((w,h));
            }

            renderer.with_value(|wrapper| {
                if let Some(wf_wrapper) = wrapper {
                    // ---- Minimal camera: fixed view-proj (replace with interactive orbit camera)
                    let aspect = if h > 0 { w as f32 / h as f32 } else { 1.0 };
                    let view_proj = fixed_viewproj(aspect);

                    wf_wrapper.0.draw(w, h, &view_proj, zgain.get(), floor_db.get(), contrast.get());
                }
            });

            if let Some(cb) = f.borrow().as_ref() {
                let _ = window_loop.request_animation_frame(cb.as_ref().unchecked_ref());
            }
        }));

        {
            let borrow = g.borrow();
            if let Some(cb) = borrow.as_ref() {
                let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
            }
        }
    });

    view! {
        <canvas node_ref=canvas_ref style="width: 100%; height: 100%; display: block;"></canvas>
    }
}

/// A simple fixed camera (you'll likely replace with your existing orbit camera math).
fn fixed_viewproj(aspect: f32) -> [f32;16] {
    // Column-major mat4
    // Perspective * LookAt (hand-coded, small)
    let fovy = 0.9_f32;
    let near = 0.05_f32;
    let far = 100.0_f32;
    let f = 1.0 / (0.5*fovy).tan();
    let nf = 1.0 / (near - far);

    let p = [
        f/aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, (far+near)*nf, -1.0,
        0.0, 0.0, (2.0*far*near)*nf, 0.0,
    ];

    // LookAt from (2.6, -1.2, 2.8) to (0,0,0.5)
    let eye = [2.6_f32, -1.2_f32, 2.8_f32];
    let center = [0.0_f32, 0.0_f32, 0.5_f32];
    let up = [0.0_f32, 1.0_f32, 0.0_f32];

    fn sub(a:[f32;3],b:[f32;3])->[f32;3]{[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
    fn dot(a:[f32;3],b:[f32;3])->f32{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
    fn cross(a:[f32;3],b:[f32;3])->[f32;3]{[a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]}
    fn norm(v:[f32;3])->[f32;3]{let l=(v[0]*v[0]+v[1]*v[1]+v[2]*v[2]).sqrt().max(1e-9); [v[0]/l,v[1]/l,v[2]/l]}
    let fwd = norm(sub(center, eye));
    let s = norm(cross(fwd, up));
    let u = cross(s, fwd);

    let v = [
        s[0], u[0], -fwd[0], 0.0,
        s[1], u[1], -fwd[1], 0.0,
        s[2], u[2], -fwd[2], 0.0,
        -dot(s, eye), -dot(u, eye), dot(fwd, eye), 1.0,
    ];

    mul4(p, v)
}

fn mul4(a:[f32;16], b:[f32;16]) -> [f32;16] {
    // column-major: o = a*b
    let mut o=[0.0_f32;16];
    for c in 0..4 {
        for r in 0..4 {
            o[c*4 + r] =
                a[0*4 + r]*b[c*4 + 0] +
                a[1*4 + r]*b[c*4 + 1] +
                a[2*4 + r]*b[c*4 + 2] +
                a[3*4 + r]*b[c*4 + 3];
        }
    }
    o
}
