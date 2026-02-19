//! Leptos component wrapper for the WebGL 3D waterfall spectrogram.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext};
use std::rc::Rc;

use crate::webgl::waterfall_renderer::Waterfall3D;
use crate::types::SpectrogramData;
use crate::dsp::bandpass::BandpassParams;
use crate::state::{AppState, BandpassMode};

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
    let state = expect_context::<AppState>();
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();

    // renderer state: StoredValue because WebGL context is !Send
    let renderer = store_value(None::<SendWrapper<Waterfall3D>>);
    let renderer_ready = create_rw_signal(false);
    let last_dims = create_rw_signal::<(i32,i32)>( (0,0) );

    // Interaction state
    let is_dragging = create_rw_signal(false);
    let last_mouse_pos = create_rw_signal::<(i32,i32)>( (0,0) );
    let drag_mode = create_rw_signal(0); // 0=rotate, 1=pan

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

    // Event handlers
    let on_mousedown = move |ev: web_sys::MouseEvent| {
        is_dragging.set(true);
        last_mouse_pos.set((ev.client_x(), ev.client_y()));
        // Middle button (1) or shift key for pan
        if ev.button() == 1 || ev.shift_key() {
            drag_mode.set(1);
        } else {
            drag_mode.set(0);
        }
    };

    let on_mouseup = move |_: web_sys::MouseEvent| {
        is_dragging.set(false);
    };

    let on_mouseleave = move |_: web_sys::MouseEvent| {
        is_dragging.set(false);
    };

    let on_mousemove = move |ev: web_sys::MouseEvent| {
        if !is_dragging.get() { return; }

        let (lx, ly) = last_mouse_pos.get();
        let cx = ev.client_x();
        let cy = ev.client_y();
        let dx = cx - lx;
        let dy = cy - ly;
        last_mouse_pos.set((cx, cy));

        if drag_mode.get() == 0 {
            // Rotate
            let sensitivity = 0.01;
            state.camera_yaw.update(|y| *y -= dx as f32 * sensitivity);
            state.camera_pitch.update(|p| {
                *p = (*p - dy as f32 * sensitivity).clamp(0.1, std::f32::consts::PI - 0.1);
            });
        } else {
            // Pan
            // We need to move target relative to camera view
            let sensitivity = 0.005 * state.camera_distance.get();
            // This is a simplified pan (just moves X/Z in world, ignores camera rotation for simplicity or should be relative?)
            // Relative is better.
            // Right vector, Up vector...
            // Let's just do simple world axis pan for now, or improve if time.
            // Actually, usually Pan moves the target perpendicular to Look vector.
            // Let's stick to X/Y pan in screen space -> X/Z in world space?
            // "Click camera move" -> assuming translation.

            // Just mapping dx to X and dy to Y (or Z)
            state.camera_target.update(|t| {
                t[0] -= dx as f32 * sensitivity;
                t[2] -= dy as f32 * sensitivity; // Z is depth/height in 3D usually, here Y is freq (height), X is time, Z is magnitude?
                // Vertex shader: x=time, y=freq, z=mag.
                // So Camera Up is likely Y.
                // So Pan X moves Time, Pan Y moves Freq?
                // Let's assume standard camera control:
                // Shift-Drag moves target.
            });
            // Let's refine Pan later if needed.
        }
    };

    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let delta = ev.delta_y() as f32;
        let zoom_speed = 0.001;
        state.camera_distance.update(|d| {
            *d = (*d * (1.0 + delta * zoom_speed)).max(0.1).min(200.0);
        });
    };

    // Touch support
    let on_touchstart = move |ev: web_sys::TouchEvent| {
        if ev.touches().length() == 1 {
            if let Some(t) = ev.touches().get(0) {
                is_dragging.set(true);
                last_mouse_pos.set((t.client_x(), t.client_y()));
                drag_mode.set(0); // Rotate
            }
        }
        // Pinch zoom could be added here
    };

    let on_touchmove = move |ev: web_sys::TouchEvent| {
        if !is_dragging.get() { return; }
        if ev.touches().length() == 1 {
             if let Some(t) = ev.touches().get(0) {
                 let (lx, ly) = last_mouse_pos.get();
                 let cx = t.client_x();
                 let cy = t.client_y();
                 let dx = cx - lx;
                 let dy = cy - ly;
                 last_mouse_pos.set((cx, cy));

                 let sensitivity = 0.01;
                 state.camera_yaw.update(|y| *y -= dx as f32 * sensitivity);
                 state.camera_pitch.update(|p| {
                    *p = (*p - dy as f32 * sensitivity).clamp(0.1, std::f32::consts::PI - 0.1);
                 });
             }
        }
    };

    let on_touchend = move |_: web_sys::TouchEvent| {
        is_dragging.set(false);
    };

    // animation loop
    create_effect(move |_| {
        let Some(canvas) = canvas_ref.get() else { return; };
        let window = web_sys::window().unwrap();
        let window_loop = window.clone();

        let f = Rc::new(std::cell::RefCell::new(None::<Closure<dyn FnMut()>>));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
            // Resize to match layout
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

            // Calc ViewProj
            let yaw = state.camera_yaw.get_untracked();
            let pitch = state.camera_pitch.get_untracked();
            let dist = state.camera_distance.get_untracked();
            let target = state.camera_target.get_untracked();

            let aspect = if h > 0 { w as f32 / h as f32 } else { 1.0 };
            let view_proj = orbit_viewproj(aspect, yaw, pitch, dist, target);

            // Calc texture offset/scale for animation
            let mut tex_offset = 0.0;
            let mut tex_scale = 1.0;

            if let Some(d) = data.get_untracked() {
                let total_duration = d.columns.len() as f64 * d.time_resolution;
                if total_duration > 0.0 {
                    let window_dur = state.waterfall_time_window.get_untracked();

                    if window_dur >= total_duration {
                        tex_scale = 1.0;
                        tex_offset = 0.0;
                    } else {
                        tex_scale = window_dur / total_duration;

                        // Center window on playhead? Or start at playhead?
                        // "view perspective default so you can see a range... defaults to 5 seconds"
                        // Usually waterfall scrolls playhead.
                        // Let's put playhead at 10% or center?
                        // If playing, we scroll.

                        let current_time = state.playhead_time.get_untracked();

                        // Let's say we want to show [current_time, current_time + window]?
                        // Or [current_time - window/2, current_time + window/2]?
                        // Often waterfall shows history (past).
                        // Let's assume standard left-to-right scrolling:
                        // Left edge = current_time.
                        // Wait, if it's "Acceleration up or down", we are looking at a spectrogram.
                        // If playing, the "cursor" moves, or the "paper" moves?
                        // "Animate the waterfall" -> Paper moves.
                        // So the view window moves.

                        // Let's define the window start.
                        let start_time = current_time - (window_dur * 0.1); // Keep playhead slightly from left edge
                        let start_uv = start_time / total_duration;

                        tex_offset = start_uv;

                        // Clamp?
                        // If we loop, we might want to wrap.
                        // Texture is CLAMP_TO_EDGE.
                        // Let's not clamp tightly, allowing seeing empty space if we go past end is fine (it will streak).
                    }
                }
            }

            renderer.with_value(|wrapper| {
                if let Some(wf_wrapper) = wrapper {
                    wf_wrapper.0.draw(
                        w, h,
                        &view_proj,
                        zgain.get_untracked(),
                        floor_db.get_untracked(),
                        contrast.get_untracked(),
                        tex_offset as f32,
                        tex_scale as f32,
                        state.waterfall_show_accel.get_untracked()
                    );
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
        <canvas node_ref=canvas_ref
            style="width: 100%; height: 100%; display: block; touch-action: none;"
            on:mousedown=on_mousedown
            on:mouseup=on_mouseup
            on:mouseleave=on_mouseleave
            on:mousemove=on_mousemove
            on:wheel=on_wheel
            on:touchstart=on_touchstart
            on:touchmove=on_touchmove
            on:touchend=on_touchend
        ></canvas>
    }
}

fn orbit_viewproj(aspect: f32, yaw: f32, pitch: f32, dist: f32, target: [f32;3]) -> [f32;16] {
    let fovy = 0.9_f32; // ~50 degrees
    let near = 0.05_f32;
    let far = 1000.0_f32;
    let f = 1.0 / (0.5*fovy).tan();
    let nf = 1.0 / (near - far);

    // Perspective matrix
    let p = [
        f/aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, (far+near)*nf, -1.0,
        0.0, 0.0, (2.0*far*near)*nf, 0.0,
    ];

    // Orbit Camera position
    // pitch is angle from Up vector (Y axis). 0 = Up, PI = Down.
    // yaw is rotation around Y axis.
    let y = pitch.cos();
    let r_xz = pitch.sin();
    let x = r_xz * yaw.sin();
    let z = r_xz * yaw.cos();

    let eye = [
        target[0] + x * dist,
        target[1] + y * dist,
        target[2] + z * dist
    ];

    // LookAt
    // Up vector is Y usually.
    let up = [0.0_f32, 1.0_f32, 0.0_f32];

    fn sub(a:[f32;3],b:[f32;3])->[f32;3]{[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
    fn dot(a:[f32;3],b:[f32;3])->f32{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
    fn cross(a:[f32;3],b:[f32;3])->[f32;3]{[a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]}
    fn norm(v:[f32;3])->[f32;3]{let l=(v[0]*v[0]+v[1]*v[1]+v[2]*v[2]).sqrt().max(1e-9); [v[0]/l,v[1]/l,v[2]/l]}

    let fwd = norm(sub(target, eye));
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
