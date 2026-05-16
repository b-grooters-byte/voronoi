use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, DrawingArea};
use gtk4::cairo;
use std::cell::RefCell;
use std::cmp;
use std::rc::Rc;


pub mod voronoi;

pub use voronoi::*;

fn main() {

    // create a new application
    let app = Application::new(Some("org.bytetrail.dtx"), Default::default());
    app.connect_activate(move |app| {
        build_ui(app);
    });
    // run the application
    app.run();
}


const WINDOW_INIT_WIDTH: i32 = 600;
const WINDOW_INIT_HEIGHT: i32 = 400;



/// Builds the GTK UI with drawing area.
fn build_ui(app: &Application) {
    let mut voronoi = Voronoi::new_random(10, 
        WINDOW_INIT_WIDTH, 
        WINDOW_INIT_HEIGHT);
    // create the window
    let window = ApplicationWindow::new(app);
    window.set_title(Some("Directrix"));
    window.set_default_size(WINDOW_INIT_WIDTH, WINDOW_INIT_HEIGHT);
    let canvas = DrawingArea::new();
    canvas.set_content_width(WINDOW_INIT_WIDTH);
    canvas.set_content_height(WINDOW_INIT_HEIGHT);
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);

    // create a reference counting smart pointer so that site may be passed to
    // to each event closure. These all occur on the UI thread so Arc not
    // necessary
    let voronoi_rc = Rc::new(RefCell::new(voronoi));
    let voronoi_clone = Rc::clone(&voronoi_rc);
    // handle the draw request for DrawingArea
    canvas.set_draw_func(move |area, ctx, width, height| {
        let v = voronoi_clone.borrow();
        v.draw(width, height, ctx);
    });

    // Mouse event controllers for GTK4
    //let voronoi_clone = Rc::clone(&voronoi_rc);
    //let click_canvas = canvas.clone();
    //let click_controller = gtk4::GestureClick::new();
    // click_controller.connect_released(move |_, n_press, x, y| {
    //     if n_press == 1 {
    //         let mut s = site_clone.borrow_mut();
    //         s.x = x;
    //         s.y = y;
    //         click_canvas.queue_draw();
    //     }
    // });
    // canvas.add_controller(click_controller);

    let voronoi_clone = Rc::clone(&voronoi_rc);
    let motion_canvas = canvas.clone();
    let motion_controller = gtk4::EventControllerMotion::new();
    motion_controller.connect_motion(move |_, _x, y| {
        let mut v = voronoi_clone.borrow_mut();        
        v.directrix = y;
        motion_canvas.queue_draw();
    });
    canvas.add_controller(motion_controller);

    window.set_child(Some(&canvas));
    window.present();
}
