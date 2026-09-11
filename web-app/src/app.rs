use gloo_timers::future::TimeoutFuture;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlInputElement, ResizeObserver};
use yew::prelude::*;

use voronoi_core::{Voronoi, lloyd};

use crate::canvas;

const CANVAS_WIDTH: i32 = 800;
const CANVAS_HEIGHT: i32 = 600;
const DEFAULT_SITES: usize = 25;
const MIN_SITES: usize = 5;
const MAX_SITES: usize = 2500;
const DEFAULT_PASSES: usize = 3;
const MIN_PASSES: usize = 1;
const MAX_PASSES: usize = 20;

/// Cancellation token for the animated sweep: a fresh one is created each
/// time Start runs, and the previous one (if any) is flipped to `true` so
/// an in-flight sweep loop notices it's been superseded and stops without
/// touching state a newer loop (or Stop/Reset) now owns. Mirrors
/// `voronoi-gtk`'s `Rc<RefCell<Arc<AtomicBool>>>` pattern, simplified
/// since wasm is single-threaded (no `Arc`/atomics needed).
type StopToken = Rc<RefCell<Rc<Cell<bool>>>>;

/// Draws at the canvas element's *current* size (its `width`/`height`
/// attributes — the drawing-buffer resolution, kept in sync with the
/// element's actual on-screen size by the resize handler below), not a
/// fixed constant, since the canvas is now flex-sized to fill whatever
/// space is available.
fn redraw(voronoi: &Rc<RefCell<Voronoi>>, canvas_ref: &NodeRef) {
    let Some(canvas) = canvas_ref.cast::<HtmlCanvasElement>() else {
        return;
    };
    let Ok(Some(ctx)) = canvas.get_context("2d") else {
        return;
    };
    let Ok(ctx) = ctx.dyn_into::<CanvasRenderingContext2d>() else {
        return;
    };
    let v = voronoi.borrow();
    canvas::draw(&v, canvas.width() as f64, canvas.height() as f64, &ctx);
}

// Speed 1 (Fast) -> 5 ms/step, Speed 10 (Slow) -> 50 ms/step. Matches
// voronoi-gtk's `speed_scale.value() as u64 * 5`.
fn delay_ms_for_speed(speed: u32) -> u32 {
    speed * 5
}

#[function_component(App)]
pub fn app() -> Html {
    let voronoi = use_mut_ref(|| Voronoi::new_random(DEFAULT_SITES, CANVAS_WIDTH, CANVAS_HEIGHT));
    let canvas_ref = use_node_ref();
    let stop_token: StopToken = use_mut_ref(|| Rc::new(Cell::new(true)));

    let sweeping = use_state(|| false);
    let finished = use_state(|| false);
    let sites_count = use_state(|| DEFAULT_SITES);
    let passes = use_state(|| DEFAULT_PASSES);
    let speed = use_state(|| 5u32);

    // Draw once the canvas element actually exists in the DOM.
    {
        let voronoi = voronoi.clone();
        let canvas_ref = canvas_ref.clone();
        use_effect_with((), move |_| {
            redraw(&voronoi, &canvas_ref);
            || ()
        });
    }

    // Keep the canvas's drawing-buffer resolution in sync with its
    // flex-determined on-screen size, and treat that the same way
    // voronoi-gtk's `connect_resize` treats a window resize: cancel any
    // running sweep, reset to a fresh (unfinished) tessellation at the
    // new size, and redraw. ResizeObserver (rather than a window resize
    // listener) is what's needed here since the canvas's size is driven
    // by flex layout, not just the window.
    {
        let voronoi = voronoi.clone();
        let stop_token = stop_token.clone();
        let sweeping = sweeping.clone();
        let finished = finished.clone();
        use_effect_with(canvas_ref.clone(), move |canvas_ref| {
            let observer_cell: Rc<RefCell<Option<ResizeObserver>>> = Rc::new(RefCell::new(None));
            if let Some(canvas_el) = canvas_ref.cast::<HtmlCanvasElement>() {
                let canvas_ref = canvas_ref.clone();
                let on_resize = Closure::<dyn FnMut()>::new(move || {
                    let Some(canvas_el) = canvas_ref.cast::<HtmlCanvasElement>() else {
                        return;
                    };
                    let new_width = canvas_el.client_width().max(100);
                    let new_height = canvas_el.client_height().max(100);
                    if canvas_el.width() as i32 == new_width && canvas_el.height() as i32 == new_height
                    {
                        return; // no real size change
                    }
                    canvas_el.set_width(new_width as u32);
                    canvas_el.set_height(new_height as u32);

                    stop_token.borrow().set(true);
                    sweeping.set(false);
                    finished.set(false);
                    voronoi.borrow_mut().resize(new_width, new_height);
                    redraw(&voronoi, &canvas_ref);
                });
                if let Ok(observer) = ResizeObserver::new(on_resize.as_ref().unchecked_ref()) {
                    observer.observe(&canvas_el);
                    *observer_cell.borrow_mut() = Some(observer);
                }
                // The closure must outlive the observer; the observer
                // itself lives in `observer_cell`, disconnected below.
                on_resize.forget();
            }
            move || {
                if let Some(observer) = observer_cell.borrow_mut().take() {
                    observer.disconnect();
                }
            }
        });
    }

    let onclick_start = {
        let voronoi = voronoi.clone();
        let canvas_ref = canvas_ref.clone();
        let stop_token = stop_token.clone();
        let sweeping = sweeping.clone();
        let finished = finished.clone();
        let speed = speed.clone();
        Callback::from(move |_: MouseEvent| {
            // Cancel any running sweep and issue a fresh cancellation token.
            stop_token.borrow().set(true);
            let my_flag = Rc::new(Cell::new(false));
            *stop_token.borrow_mut() = my_flag.clone();

            let height = {
                let mut v = voronoi.borrow_mut();
                v.start_sweep();
                v.height as f64
            };
            sweeping.set(true);
            finished.set(false);

            let voronoi = voronoi.clone();
            let canvas_ref = canvas_ref.clone();
            let sweeping = sweeping.clone();
            let finished = finished.clone();
            let speed = speed.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut y = 0.0_f64;
                loop {
                    if my_flag.get() || y > height {
                        break;
                    }
                    voronoi.borrow_mut().advance_to(y);
                    redraw(&voronoi, &canvas_ref);
                    y += 1.0;
                    // Re-read on every tick (rather than once at Start) so
                    // a mid-sweep Speed change takes effect immediately.
                    TimeoutFuture::new(delay_ms_for_speed(*speed)).await;
                }
                // Only finish up if nothing (Stop/Reset/a newer Start)
                // superseded this loop while it was running.
                if !my_flag.get() {
                    voronoi.borrow_mut().finish_tessellation();
                    redraw(&voronoi, &canvas_ref);
                    sweeping.set(false);
                    finished.set(true);
                }
            });
        })
    };

    let onclick_stop = {
        let stop_token = stop_token.clone();
        let sweeping = sweeping.clone();
        Callback::from(move |_: MouseEvent| {
            stop_token.borrow().set(true);
            sweeping.set(false);
        })
    };

    let onclick_reset = {
        let stop_token = stop_token.clone();
        let voronoi = voronoi.clone();
        let canvas_ref = canvas_ref.clone();
        let sweeping = sweeping.clone();
        let finished = finished.clone();
        Callback::from(move |_: MouseEvent| {
            stop_token.borrow().set(true);
            sweeping.set(false);
            finished.set(false);
            // Rebuilding (rather than just clearing) the beachline puts it
            // back into the same "ready to run" state Start expects.
            voronoi.borrow_mut().start_sweep();
            redraw(&voronoi, &canvas_ref);
        })
    };

    let oninput_sites = {
        let voronoi = voronoi.clone();
        let canvas_ref = canvas_ref.clone();
        let sites_count = sites_count.clone();
        let finished = finished.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            if let Ok(value) = input.value().parse::<usize>() {
                let clamped = value.clamp(MIN_SITES, MAX_SITES);
                sites_count.set(clamped);
                voronoi.borrow_mut().regenerate(clamped);
                finished.set(false);
                redraw(&voronoi, &canvas_ref);
            }
        })
    };

    let oninput_speed = {
        let speed = speed.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            if let Ok(value) = input.value().parse::<u32>() {
                speed.set(value);
            }
        })
    };

    let oninput_passes = {
        let passes = passes.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            if let Ok(value) = input.value().parse::<usize>() {
                passes.set(value.clamp(MIN_PASSES, MAX_PASSES));
            }
        })
    };

    let onclick_relax = {
        let voronoi = voronoi.clone();
        let canvas_ref = canvas_ref.clone();
        let passes = *passes;
        Callback::from(move |_: MouseEvent| {
            lloyd::relax(&mut voronoi.borrow_mut(), passes);
            redraw(&voronoi, &canvas_ref);
        })
    };

    html! {
        <div style="display: flex; flex-direction: row; font-family: sans-serif; height: 100vh; width: 100vw;">
            <div class="sidebar">
                <label>{ "Sites" }</label>
                <input
                    type="number"
                    min={MIN_SITES.to_string()}
                    max={MAX_SITES.to_string()}
                    value={sites_count.to_string()}
                    disabled={*sweeping}
                    oninput={oninput_sites}
                />
                <hr style="width: 100%;" />

                <label>{ "Speed" }</label>
                <div style="display: flex; align-items: center; gap: 4px;">
                    <span>{ "Fast" }</span>
                    <input
                        type="range" min="1" max="10"
                        value={speed.to_string()}
                        oninput={oninput_speed}
                        style="flex: 1;"
                    />
                    <span>{ "Slow" }</span>
                </div>
                <hr style="width: 100%;" />

                <button onclick={onclick_start}>{ "Start" }</button>
                <button onclick={onclick_stop}>{ "Stop" }</button>
                <button onclick={onclick_reset}>{ "Reset" }</button>
                <hr style="width: 100%;" />

                <label>{ "Lloyd's Passes" }</label>
                <input
                    type="number"
                    min={MIN_PASSES.to_string()}
                    max={MAX_PASSES.to_string()}
                    value={passes.to_string()}
                    oninput={oninput_passes}
                />
                <button onclick={onclick_relax} disabled={!*finished}>{ "Relax" }</button>
            </div>
            <canvas
                ref={canvas_ref}
                width={CANVAS_WIDTH.to_string()}
                height={CANVAS_HEIGHT.to_string()}
                style="flex: 1; min-width: 0; display: block; width: 100%; height: 100%; border-left: 1px solid #ccc; box-sizing: border-box;"
            />
        </div>
    }
}
