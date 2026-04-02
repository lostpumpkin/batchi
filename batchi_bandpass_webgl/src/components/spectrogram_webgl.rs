//! Leptos component wrapper for the WebGL 3D waterfall spectrogram.
//!
//! This is designed to live alongside your existing Canvas2D spectrogram view.
//! You can gate it behind a "View Mode" toggle:
//!   - 2D Spectrogram (existing)
//!   - SyllableTrace (Canvas2D ribbon; previous integration)
//!   - WebGL Waterfall 3D (this)
//!
//! This component:
//! - creates a <canvas>
//! - initializes WebGL2 + Waterfall3D renderer on mount
//! - exposes a hook to upload spectrogram data when it changes
//!
//! You will need to adapt the `SpectrogramData` accessors to your actual types.

use leptos::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext};

use crate::webgl::waterfall_renderer::Waterfall3D;

// ---- TODO: replace this with your actual SpectrogramData type.
#[allow(dead_code)]
pub struct SpectrogramDataStub {
    pub t_bins: usize,
    pub f_bins: usize,
    pub db: Vec<f32>, // len=t_bins*f_bins, time-major
}

#[component]
pub fn SpectrogramWebGl3D(
    /// Provide the latest spectrogram data (signals are typical in Batchi).
    /// Replace this stub with: ReadSignal<Option<SpectrogramData>> or similar.
    data: ReadSignal<Option<SpectrogramDataStub>>,
    /// Visual params
    zgain: ReadSignal<f32>,
    floor_db: ReadSignal<f32>,
    contrast: ReadSignal<f32>,
) -> impl IntoView {
    let canvas_ref = create_node_ref::<HtmlCanvasElement>();

    // renderer state
    let renderer = create_rw_signal::<Option<Waterfall3D>>(None);
    let last_dims = create_rw_signal::<(i32,i32)>( (0,0) );

    // init on mount
    create_effect(move |_| {
        let Some(canvas) = canvas_ref.get() else { return; };
        let gl: WebGl2RenderingContext = canvas
            .get_context("webgl2").ok().flatten()
            .and_then(|c| c.dyn_into::<WebGl2RenderingContext>().ok())
            .expect("WebGL2 unavailable");

        let mut wf = Waterfall3D::new(gl).expect("failed to init Waterfall3D");
        renderer.set(Some(wf));

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
        let Some(d) = data.get() else { return; };
        let Some(mut wf) = renderer.get() else { return; };
        // Upload (dB grid)
        let _ = wf.upload_db_texture(d.t_bins as i32, d.f_bins as i32, &d.db);
        renderer.set(Some(wf));
    });

    // animation loop
    create_effect(move |_| {
        let Some(canvas) = canvas_ref.get() else { return; };
        let window = web_sys::window().unwrap();

        let f = Rc::new(std::cell::RefCell::new(None));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
            // Resize to match layout (cheap)
            let dpr = window.device_pixel_ratio().min(2.0).max(1.0);
            let rect = canvas.get_bounding_client_rect();
            let w = (rect.width() * dpr).floor() as i32;
            let h = (rect.height() * dpr).floor() as i32;
            let (lw, lh) = last_dims.get_untracked();
            if w != lw || h != lh {
                canvas.set_width(w as u32);
                canvas.set_height(h as u32);
                last_dims.set((w,h));
            }

            let Some(wf) = renderer.get_untracked() else {
                window.request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref()).ok();
                return;
            };

            // ---- Minimal camera: fixed view-proj (replace with interactive orbit camera)
            let aspect = if h > 0 { w as f32 / h as f32 } else { 1.0 };
            let view_proj = fixed_viewproj(aspect);

            wf.draw(w, h, &view_proj, zgain.get_untracked(), floor_db.get_untracked(), contrast.get_untracked());

            window.request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref()).ok();
        }));

        window.request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref()).ok();
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
