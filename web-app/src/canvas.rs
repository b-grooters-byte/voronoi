//! Canvas2D rendering for `voronoi_core::Voronoi`. Mirrors
//! `voronoi-gtk`'s `render.rs` function-for-function, swapping
//! `cairo::Context` calls for `CanvasRenderingContext2d` ones — all
//! geometry (edges, beachline curve, clamping) still comes from the core
//! crate's public data-producing methods.

use voronoi_core::Voronoi;
use web_sys::CanvasRenderingContext2d;

fn rgb_style(color: (f64, f64, f64)) -> String {
    let (r, g, b) = color;
    format!(
        "rgb({}, {}, {})",
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// Renders the current state of `v` onto `ctx`: site markers, and — only
/// while the sweep is still in progress — the directrix readout, sweep
/// line, and transient beachline. Cell edges are always drawn.
pub fn draw(v: &Voronoi, width: f64, height: f64, ctx: &CanvasRenderingContext2d) {
    ctx.clear_rect(0.0, 0.0, width, height);
    draw_sites(v, ctx);
    if !v.is_finished() {
        draw_directrix(v, width, ctx);
        draw_beachline(v, ctx);
    }
    draw_edges(v, ctx);
}

fn draw_sites(v: &Voronoi, ctx: &CanvasRenderingContext2d) {
    ctx.set_stroke_style_str("red");
    ctx.set_line_width(1.0);
    for site in &v.sites {
        ctx.begin_path();
        let _ = ctx.arc(site.x, site.y, 2.0, 0.0, 2.0 * std::f64::consts::PI);
        ctx.stroke();
    }
}

fn draw_directrix(v: &Voronoi, width: f64, ctx: &CanvasRenderingContext2d) {
    ctx.set_fill_style_str("black");
    ctx.set_font("12px monospace");
    let _ = ctx.fill_text(&format!("Directrix: {}", v.directrix), 8.0, 34.0);

    ctx.set_stroke_style_str("black");
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(0.0, v.directrix);
    ctx.line_to(width, v.directrix);
    ctx.stroke();
}

fn draw_beachline(v: &Voronoi, ctx: &CanvasRenderingContext2d) {
    ctx.set_line_width(1.0);
    for (site_idx, points) in v.beachline_curve() {
        let color = v.sites[site_idx].color;
        ctx.set_stroke_style_str(&rgb_style(color));
        ctx.begin_path();
        for (i, p) in points.iter().enumerate() {
            if i == 0 {
                ctx.move_to(p.x, p.y);
            } else {
                ctx.line_to(p.x, p.y);
            }
        }
        ctx.stroke();
    }
}

fn draw_edges(v: &Voronoi, ctx: &CanvasRenderingContext2d) {
    ctx.set_stroke_style_str("black");
    ctx.set_line_width(1.0);
    ctx.begin_path();
    for (_, _, start, end, _done) in v.all_edge_segments() {
        ctx.move_to(start.x, start.y);
        ctx.line_to(end.x, end.y);
    }
    ctx.stroke();
}
