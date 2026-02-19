use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use crate::state::AppState;
use crate::audio::playback;
use crate::components::file_sidebar::FileSidebar;
use crate::components::spectrogram::Spectrogram;
use crate::components::waveform::Waveform;
use crate::components::toolbar::Toolbar;
use crate::components::analysis_panel::AnalysisPanel;

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state);

    // Global keyboard shortcut: Space = play/stop
    let state_kb = state.clone();
    let handler = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
        // Ignore if focus is on an input/select/textarea
        if let Some(target) = ev.target() {
            if let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() {
                let tag = el.tag_name();
                if tag == "INPUT" || tag == "SELECT" || tag == "TEXTAREA" {
                    return;
                }
            }
        }
        if ev.key() == " " {
            ev.prevent_default();
            if state_kb.current_file_index.get_untracked().is_some() {
                if state_kb.is_playing.get_untracked() {
                    playback::stop(&state_kb);
                } else {
                    playback::play(&state_kb);
                }
            }
        }
    });
    let window = web_sys::window().unwrap();
    let _ = window.add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
    handler.forget();

    // Responsive / Mobile detection
    let w_inner = window.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1024.0);
    if w_inner < 768.0 {
        state.is_mobile.set(true);
        state.sidebar_collapsed.set(true);
    }

    let state_resize = state.clone();
    let resize_handler = Closure::<dyn Fn(web_sys::Event)>::new(move |_| {
        let w = web_sys::window().unwrap();
        let width = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1024.0);
        let was_mobile = state_resize.is_mobile.get_untracked();
        let is_mobile = width < 768.0;

        if is_mobile != was_mobile {
            state_resize.is_mobile.set(is_mobile);
            if is_mobile {
                state_resize.sidebar_collapsed.set(true);
            }
        }
    });
    let _ = window.add_event_listener_with_callback("resize", resize_handler.as_ref().unchecked_ref());
    resize_handler.forget();

    let grid_style = move || {
        if state.is_mobile.get() {
             if state.sidebar_collapsed.get() {
                 "grid-template-columns: 0px 1fr".to_string()
             } else {
                 "grid-template-columns: 1fr 0px".to_string() // Show sidebar effectively full width if we hide main, but logic elsewhere handles it?
                 // Actually sidebar width is controlled by sidebar_width.
                 // For mobile, we might want sidebar to take full width or be an overlay.
                 // Simplest: same grid, but adjust sidebar_width or main display in CSS.
             }
        } else {
            if state.sidebar_collapsed.get() {
                "grid-template-columns: 0px 1fr".to_string()
            } else {
                format!("grid-template-columns: {}px 1fr", state.sidebar_width.get() as i32)
            }
        }
    };

    let app_class = move || {
        if state.is_mobile.get() { "app mobile-mode" } else { "app" }
    };

    view! {
        <div class=app_class style=grid_style>
            <FileSidebar />
            <MainArea />
        </div>
    }
}

#[component]
fn MainArea() -> impl IntoView {
    let state = expect_context::<AppState>();
    let has_file = move || state.current_file_index.get().is_some();

    view! {
        <div class="main">
            <Toolbar />
            {move || {
                if has_file() {
                    view! {
                        <Spectrogram />
                        <Waveform />
                        <AnalysisPanel />
                    }.into_any()
                } else {
                    view! {
                        <div class="empty-state">
                            "Drop WAV or FLAC files into the sidebar"
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
