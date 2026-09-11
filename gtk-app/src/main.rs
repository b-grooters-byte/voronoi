use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, DrawingArea, Orientation};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use voronoi_core::{Voronoi, lloyd};

mod render;

const WINDOW_INIT_WIDTH: i32 = 900;
const WINDOW_INIT_HEIGHT: i32 = 500;
const CANVAS_WIDTH: i32 = 800;
const CANVAS_HEIGHT: i32 = 600;
const DEFAULT_SITES: f64 = 25.0;
const MAX_SITES: f64 = 2500.0;
const MIN_SITES: f64 = 5.0;
const DEFAULT_PASSES: f64 = 3.0;
const MAX_PASSES: f64 = 20.0;
const MIN_PASSES: f64 = 1.0;

fn main() {
    let app = Application::new(Some("org.bag.voronoi"), Default::default());
    app.connect_activate(move |app| {
        build_ui(app);
    });
    app.run();
}

fn build_ui(app: &Application) {
    let voronoi = Voronoi::new_random(DEFAULT_SITES as usize, CANVAS_WIDTH, CANVAS_HEIGHT);
    let window = ApplicationWindow::new(app);
    window.set_title(Some("Voronoi Diagram"));
    window.set_default_size(WINDOW_INIT_WIDTH, WINDOW_INIT_HEIGHT);

    let root = gtk4::Box::new(Orientation::Horizontal, 0);

    let controls = gtk4::Box::new(Orientation::Vertical, 8);
    controls.set_margin_top(16);
    controls.set_margin_bottom(16);
    controls.set_margin_start(12);
    controls.set_margin_end(12);
    controls.set_width_request(180);

    let sites_label = gtk4::Label::new(Some("Sites"));
    sites_label.set_halign(gtk4::Align::Start);
    let sites_spin = gtk4::SpinButton::with_range(MIN_SITES, MAX_SITES, 1.0);
    sites_spin.set_value(DEFAULT_SITES);
    sites_spin.set_hexpand(true);

    controls.append(&sites_label);
    controls.append(&sites_spin);
    controls.append(&gtk4::Separator::new(Orientation::Horizontal));

    let speed_label = gtk4::Label::new(Some("Speed"));
    speed_label.set_halign(gtk4::Align::Start);
    let speed_row = gtk4::Box::new(Orientation::Horizontal, 4);
    let speed_scale = gtk4::Scale::with_range(Orientation::Horizontal, 1.0, 10.0, 1.0);
    speed_scale.set_draw_value(false);
    speed_scale.set_value(5.0);
    speed_scale.set_hexpand(true);
    speed_row.append(&gtk4::Label::new(Some("Fast")));
    speed_row.append(&speed_scale);
    speed_row.append(&gtk4::Label::new(Some("Slow")));

    controls.append(&speed_label);
    controls.append(&speed_row);
    controls.append(&gtk4::Separator::new(Orientation::Horizontal));

    let start_btn = gtk4::Button::with_label("Start");
    let stop_btn = gtk4::Button::with_label("Stop");
    let reset_btn = gtk4::Button::with_label("Reset");
    start_btn.set_hexpand(true);
    stop_btn.set_hexpand(true);
    reset_btn.set_hexpand(true);

    controls.append(&start_btn);
    controls.append(&stop_btn);
    controls.append(&reset_btn);
    controls.append(&gtk4::Separator::new(Orientation::Horizontal));

    let passes_label = gtk4::Label::new(Some("Lloyd's Passes"));
    passes_label.set_halign(gtk4::Align::Start);
    let passes_spin = gtk4::SpinButton::with_range(MIN_PASSES, MAX_PASSES, 1.0);
    passes_spin.set_value(DEFAULT_PASSES);
    passes_spin.set_hexpand(true);

    // Only meaningful once a tessellation is complete — cell shapes (and
    // so centroids) aren't well-defined mid-sweep.
    let relax_btn = gtk4::Button::with_label("Relax");
    relax_btn.set_hexpand(true);
    relax_btn.set_sensitive(false);

    controls.append(&passes_label);
    controls.append(&passes_spin);
    controls.append(&relax_btn);

    root.append(&controls);
    root.append(&gtk4::Separator::new(Orientation::Vertical));

    let canvas = DrawingArea::new();
    canvas.set_content_width(CANVAS_WIDTH);
    canvas.set_content_height(CANVAS_HEIGHT);
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);

    let voronoi_rc = Rc::new(RefCell::new(voronoi));

    let voronoi_clone = Rc::clone(&voronoi_rc);
    canvas.set_draw_func(move |_area, ctx, width, height| {
        let v = voronoi_clone.borrow();
        render::draw(&v, width, height, ctx);
    });

    // tokio::sync::mpsc::Sender is Send so it moves into the Tokio task.
    // glib::spawn_future_local runs the receiver loop on the GTK main thread,
    // so it can safely capture Rc<RefCell<Voronoi>> and the canvas.
    let (tx, mut rx): (
        tokio::sync::mpsc::Sender<f64>,
        tokio::sync::mpsc::Receiver<f64>,
    ) = tokio::sync::mpsc::channel(64);

    // sweep_running: true while a Tokio sweep task is active.
    // Used to gate sites_spin sensitivity and ignore stale Done signals.
    let sweep_running: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    // The sweep task (a background Tokio task, not on the GTK main thread)
    // reads this on every tick instead of capturing a fixed delay once at
    // Start, so dragging the Speed slider takes effect immediately on an
    // already-running sweep, not just on the next Start.
    let speed_value = Arc::new(AtomicU64::new(speed_scale.value() as u64));

    let voronoi_clone = Rc::clone(&voronoi_rc);
    let canvas_rx = canvas.clone();
    let sites_spin_rx = sites_spin.clone();
    let relax_btn_rx = relax_btn.clone();
    let sweep_running_rx = Rc::clone(&sweep_running);
    glib::spawn_future_local(async move {
        while let Some(y) = rx.recv().await {
            if y == f64::NEG_INFINITY {
                // Sweep task signalled natural completion; only re-enable if we
                // haven't already re-enabled via Stop/Reset.
                if sweep_running_rx.get() {
                    sweep_running_rx.set(false);
                    sites_spin_rx.set_sensitive(true);
                    // The animated sweep only covers y in [0, height], so
                    // edges on the outer hull are still short of their true
                    // endpoint; push the sweep the rest of the way (off
                    // canvas, unanimated) to finish them off.
                    voronoi_clone.borrow_mut().finish_tessellation();
                    // Cell shapes only mean anything once the tessellation
                    // is complete, so relaxation only becomes available now.
                    relax_btn_rx.set_sensitive(true);
                    canvas_rx.queue_draw();
                }
            } else {
                let mut v = voronoi_clone.borrow_mut();
                v.advance_to(y);
                canvas_rx.queue_draw();
            }
        }
    });

    let rt = Rc::new(tokio::runtime::Runtime::new().expect("failed to create tokio runtime"));

    let stop_flag: Rc<RefCell<Arc<AtomicBool>>> =
        Rc::new(RefCell::new(Arc::new(AtomicBool::new(true))));

    let stop_flag_resize = Rc::clone(&stop_flag);
    let sweep_running_resize = Rc::clone(&sweep_running);
    let sites_spin_resize = sites_spin.clone();
    let relax_btn_resize = relax_btn.clone();
    let voronoi_resize = Rc::clone(&voronoi_rc);
    let tx_resize = tx.clone();
    canvas.connect_resize(move |area, width, height| {
        stop_flag_resize.borrow().store(true, Ordering::Relaxed);
        sweep_running_resize.set(false);
        sites_spin_resize.set_sensitive(true);
        relax_btn_resize.set_sensitive(false);

        voronoi_resize.borrow_mut().resize(width, height);
        area.queue_draw();
        // Flush the directrix through the channel so any sweep values still
        // in flight from before the cancellation don't clobber the reset.
        tx_resize.try_send(0.0).ok();
    });

    let stop_flag_start = Rc::clone(&stop_flag);
    let sweep_running_start = Rc::clone(&sweep_running);
    let sites_spin_start = sites_spin.clone();
    let relax_btn_start = relax_btn.clone();
    let rt_start = Rc::clone(&rt);
    let tx_start = tx.clone();
    let speed_value_start = Arc::clone(&speed_value);
    let voronoi_start = Rc::clone(&voronoi_rc);
    start_btn.connect_clicked(move |_| {
        // Cancel any running sweep and issue a fresh cancellation token.
        stop_flag_start.borrow().store(true, Ordering::Relaxed);
        let flag = Arc::new(AtomicBool::new(false));
        *stop_flag_start.borrow_mut() = Arc::clone(&flag);
        let height = voronoi_start.borrow().height as f64;
        voronoi_start.borrow_mut().start_sweep();
        sweep_running_start.set(true);
        sites_spin_start.set_sensitive(false);
        relax_btn_start.set_sensitive(false);

        let tx = tx_start.clone();
        let speed_value = Arc::clone(&speed_value_start);

        rt_start.spawn(async move {
            let mut y = 0.0_f64;
            loop {
                if flag.load(Ordering::Relaxed) || y > height as f64 {
                    break;
                }
                if tx.send(y).await.is_err() {
                    break;
                }
                y += 1.0;
                // Speed 1 (Fast) → 5 ms/step, Speed 10 (Slow) → 50 ms/step.
                // Read fresh each tick (rather than once at Start) so a
                // mid-sweep Speed change takes effect immediately.
                let delay_ms = speed_value.load(Ordering::Relaxed) * 5;
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
            // Signal completion so the receiver can re-enable sites_spin.
            tx.send(f64::NEG_INFINITY).await.ok();
        });
    });

    //-----------------------------------------------------------------------------------
    // handle stop
    let stop_flag_stop = Rc::clone(&stop_flag);
    let sweep_running_stop = Rc::clone(&sweep_running);
    let sites_spin_stop = sites_spin.clone();
    let relax_btn_stop = relax_btn.clone();
    stop_btn.connect_clicked(move |_| {
        stop_flag_stop.borrow().store(true, Ordering::Relaxed);
        sweep_running_stop.set(false);
        sites_spin_stop.set_sensitive(true);
        relax_btn_stop.set_sensitive(false);
    });

    //-----------------------------------------------------------------------------------
    // handle reset
    let stop_flag_reset = Rc::clone(&stop_flag);
    let sweep_running_reset = Rc::clone(&sweep_running);
    let sites_spin_reset = sites_spin.clone();
    let relax_btn_reset = relax_btn.clone();
    let tx_reset = tx.clone();
    let voronoi_reset = Rc::clone(&voronoi_rc);
    reset_btn.connect_clicked(move |_| {
        stop_flag_reset.borrow().store(true, Ordering::Relaxed);
        sweep_running_reset.set(false);
        sites_spin_reset.set_sensitive(true);
        relax_btn_reset.set_sensitive(false);
        // Rebuilding (rather than just clearing) the beachline puts it back
        // into the same "ready to run" state Start expects.
        voronoi_reset.borrow_mut().start_sweep();
        tx_reset.try_send(0.0).ok();
    });

    //-----------------------------------------------------------------------------------
    // handle speed scale — keeps speed_value in sync so the in-flight
    // sweep task (see start_btn's handler) picks up changes immediately.
    let speed_value_changed = Arc::clone(&speed_value);
    speed_scale.connect_value_changed(move |scale| {
        speed_value_changed.store(scale.value() as u64, Ordering::Relaxed);
    });

    //-----------------------------------------------------------------------------------
    // handle sites spin control
    // Disabled while sweeping; changing the count regenerates sites and
    // auto-resets the directrix so the new diagram starts from scratch.
    let voronoi_clone = Rc::clone(&voronoi_rc);
    let canvas_sites = canvas.clone();
    let relax_btn_sites = relax_btn.clone();
    sites_spin.connect_value_changed(move |spin| {
        let mut v = voronoi_clone.borrow_mut();
        v.regenerate(spin.value() as usize);
        relax_btn_sites.set_sensitive(false);
        canvas_sites.queue_draw();
    });

    //-----------------------------------------------------------------------------------
    // handle relax (Lloyd's relaxation) — only enabled once a sweep has
    // completed; see the `finish_tessellation` branch above and the
    // Start/Stop/Reset/resize/sites-spin handlers that disable it again.
    let voronoi_relax = Rc::clone(&voronoi_rc);
    let canvas_relax = canvas.clone();
    let passes_spin_relax = passes_spin.clone();
    relax_btn.connect_clicked(move |_| {
        let mut v = voronoi_relax.borrow_mut();
        lloyd::relax(&mut v, passes_spin_relax.value() as usize);
        drop(v);
        canvas_relax.queue_draw();
    });

    root.append(&canvas);
    window.set_child(Some(&root));
    window.present();
}
