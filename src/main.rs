use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, DrawingArea, Orientation};
use std::cell::RefCell;
use std::rc::Rc;

pub mod voronoi;
pub use voronoi::*;

const WINDOW_INIT_WIDTH: i32 = 900;
const WINDOW_INIT_HEIGHT: i32 = 500;
const CANVAS_WIDTH: i32 = 700;
const CANVAS_HEIGHT: i32 = 500;
const DEFAULT_SITES: f64 = 15.0;

fn main() {
    let app = Application::new(Some("org.bytetrail.dtx"), Default::default());
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

    // Root horizontal container: control panel | separator | canvas
    let root = gtk4::Box::new(Orientation::Horizontal, 0);

    // ── Control panel ────────────────────────────────────────────────────────
    let controls = gtk4::Box::new(Orientation::Vertical, 8);
    controls.set_margin_top(16);
    controls.set_margin_bottom(16);
    controls.set_margin_start(12);
    controls.set_margin_end(12);
    controls.set_width_request(180);

    // Sites
    let sites_label = gtk4::Label::new(Some("Sites"));
    sites_label.set_halign(gtk4::Align::Start);
    let sites_spin = gtk4::SpinButton::with_range(5.0, 50.0, 1.0);
    sites_spin.set_value(DEFAULT_SITES);
    sites_spin.set_hexpand(true);

    controls.append(&sites_label);
    controls.append(&sites_spin);

    controls.append(&gtk4::Separator::new(Orientation::Horizontal));

    // Speed (Fast ──slider── Slow)
    let speed_label = gtk4::Label::new(Some("Speed"));
    speed_label.set_halign(gtk4::Align::Start);

    let speed_row = gtk4::Box::new(Orientation::Horizontal, 4);
    let fast_label = gtk4::Label::new(Some("Fast"));
    let slow_label = gtk4::Label::new(Some("Slow"));
    let speed_scale = gtk4::Scale::with_range(Orientation::Horizontal, 1.0, 10.0, 1.0);
    speed_scale.set_draw_value(false);
    speed_scale.set_value(5.0);
    speed_scale.set_hexpand(true);

    speed_row.append(&fast_label);
    speed_row.append(&speed_scale);
    speed_row.append(&slow_label);

    controls.append(&speed_label);
    controls.append(&speed_row);

    controls.append(&gtk4::Separator::new(Orientation::Horizontal));

    // Start / Stop / Reset
    let start_btn = gtk4::Button::with_label("Start");
    let stop_btn = gtk4::Button::with_label("Stop");
    let reset_btn = gtk4::Button::with_label("Reset");

    start_btn.set_hexpand(true);
    stop_btn.set_hexpand(true);
    reset_btn.set_hexpand(true);

    controls.append(&start_btn);
    controls.append(&stop_btn);
    controls.append(&reset_btn);

    root.append(&controls);
    root.append(&gtk4::Separator::new(Orientation::Vertical));

    // ── Drawing canvas ───────────────────────────────────────────────────────
    let canvas = DrawingArea::new();
    canvas.set_content_width(CANVAS_WIDTH);
    canvas.set_content_height(CANVAS_HEIGHT);
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);

    let voronoi_rc = Rc::new(RefCell::new(voronoi));

    let voronoi_clone = Rc::clone(&voronoi_rc);
    canvas.set_draw_func(move |_area, ctx, width, height| {
        let v = voronoi_clone.borrow();
        v.draw(width, height, ctx);
    });

    let voronoi_clone = Rc::clone(&voronoi_rc);
    let motion_canvas = canvas.clone();
    let motion_controller = gtk4::EventControllerMotion::new();
    motion_controller.connect_motion(move |_, _x, y| {
        let mut v = voronoi_clone.borrow_mut();
        v.directrix = y;
        motion_canvas.queue_draw();
    });
    canvas.add_controller(motion_controller);

    root.append(&canvas);
    window.set_child(Some(&root));
    window.present();
}
