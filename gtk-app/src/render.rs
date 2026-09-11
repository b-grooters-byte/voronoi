//! Cairo rendering for `voronoi_core::Voronoi`. All geometry (edges,
//! beachline curve, clamping degenerate coordinates) comes from the core
//! crate's public data-producing methods — this module only turns that
//! data into Cairo drawing calls.

use gtk4::cairo;
use voronoi_core::Voronoi;

/// Renders the current state of `v` onto `ctx`: site markers, and — only
/// while the sweep is still in progress — the directrix readout, sweep
/// line, and transient beachline. Cell edges are always drawn (mid-sweep
/// they're partially grown; once finished, they're the complete diagram).
pub fn draw(v: &Voronoi, width: i32, _height: i32, ctx: &cairo::Context) {
    draw_sites(v, ctx);
    if !v.is_finished() {
        draw_directrix(v, width, ctx);
        draw_beachline(v, ctx);
    }
    draw_edges(v, ctx);
}

fn draw_sites(v: &Voronoi, ctx: &cairo::Context) {
    ctx.set_source_rgba(1.0, 0.0, 0.0, 1.0);
    ctx.set_line_width(1.0);
    for site in &v.sites {
        ctx.new_path();
        ctx.arc(site.x, site.y, 2.0, 0.0, 2.0 * std::f64::consts::PI);
        if let Err(_e) = ctx.stroke() {
            // TODO handle error
        }
    }
}

fn draw_directrix(v: &Voronoi, width: i32, ctx: &cairo::Context) {
    ctx.set_source_rgba(0.0, 0.0, 0.0, 1.0);
    ctx.select_font_face(
        "Monospace",
        cairo::FontSlant::Normal,
        cairo::FontWeight::Normal,
    );
    ctx.set_font_size(12.0);
    ctx.set_line_width(1.0);
    ctx.move_to(8.0, 34.0);
    if let Err(_e) = ctx.show_text(format!("Directrix: {}", v.directrix).as_str()) {
        // TODO handle error
    }
    ctx.set_source_rgba(0.0, 0.0, 0.0, 1.0);
    ctx.move_to(0.0, v.directrix);
    ctx.line_to(width as f64, v.directrix);
    ctx.stroke().unwrap();
}

fn draw_beachline(v: &Voronoi, ctx: &cairo::Context) {
    ctx.set_line_width(1.0);
    for (site_idx, points) in v.beachline_curve() {
        let color = v.sites[site_idx].color;
        ctx.set_source_rgba(color.0, color.1, color.2, 1.0);
        ctx.new_path();
        for (i, p) in points.iter().enumerate() {
            if i == 0 {
                ctx.move_to(p.x, p.y);
            } else {
                ctx.line_to(p.x, p.y);
            }
        }
        if let Err(_e) = ctx.stroke() {
            println!("Error stroking beachline: {:?}", _e);
        }
    }
}

fn draw_edges(v: &Voronoi, ctx: &cairo::Context) {
    ctx.set_source_rgba(0.0, 0.0, 0.0, 1.0);
    ctx.set_line_width(1.0);
    ctx.new_path();
    for (_, _, start, end, _done) in v.all_edge_segments() {
        ctx.move_to(start.x, start.y);
        ctx.line_to(end.x, end.y);
    }
    if let Err(_e) = ctx.stroke() {
        println!("Error stroking cell edges: {:?}", _e);
    }
}
