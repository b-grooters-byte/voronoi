use std::cmp;

use gtk4::cairo;


pub const PARABOLA_X_STEP: usize = 5;

#[derive(Debug, Copy, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Site is a simple truct that represents the current state of a simplified
/// Voronoi site. The fields are :
/// * x - X Position
/// * y - Y Position
/// * directrix - Directrix Y
#[derive(Debug, Copy, Clone)]
pub struct Site {
    pub x: f64,
    pub y: f64,
    pub color: (f64, f64, f64),
    pub last_point: (Point, Point),
}


impl Site {
    /// Renders the current directrix based representation of a site.
    pub fn draw(&self, directrix: f64, width: i32, height: i32, ctx: &cairo::Context) {
        // get the clip region
        let clip = ctx.clip_extents().unwrap();
        // draw the origin
        ctx.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        ctx.set_line_width(1.0);
        ctx.new_path();
        ctx.arc(self.x, self.y, 2.0, 0.0, 2.0 * std::f64::consts::PI);
        if let Err(_e) = ctx.stroke() {
            // TODO handle error
        }
        // draw the parabola
        ctx.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        ctx.new_path();

        if directrix < self.y {
            return;
        }
        // used to start and stop the line_to once the arc is outside the visible window
        let mut rendering = false;
        let mut stop_render = false;
        let mut prev_x: usize = 0;
        let mut prev_y: Option<f64> = None;
        ctx.set_line_width(0.5);
        ctx.set_source_rgba(0.0, 0.0, 0.0, 0.5);
        ctx.new_path();
        let start_x = cmp::max(clip.0 as i32 - PARABOLA_X_STEP as i32, 0) as usize;
        let end_x = clip.2 as usize + PARABOLA_X_STEP;
        for x in (start_x..=end_x).step_by(PARABOLA_X_STEP) {
            let y = 1.0 / (2.0 * (self.y - directrix)) * ((x as f64 - self.x) * (x as f64 - self.x))
                + ((self.y + directrix) / 2.0);
            if rendering {
                ctx.line_to(x as f64, y);
            }
            if y > 0.0 && y < clip.3 as f64 && !rendering {
                rendering = true;
                if let Some(y) = prev_y {
                    ctx.move_to(prev_x as f64, y);
                } else {
                    ctx.move_to(x as f64, y);
                }
            } else {
                prev_x = x;
                prev_y = Some(y);
            }
            if stop_render {
                break;
            }
            stop_render = rendering && (y < 0.0 || y > height as f64);
        }
        if let Err(_e) = ctx.stroke() {
            // TODO handle error
        }
    }
}
    

#[derive(Debug, Clone)]
pub struct Voronoi {
    pub width: i32,
    pub height: i32,
    pub directrix: f64,
    pub sites: Vec<Site>,
    pub active_sites: Vec<Site>,
}

impl Voronoi {
    pub fn new(width: i32, height: i32) -> Self {
        Voronoi { width, height, directrix: 0.0, sites: Vec::new(), active_sites: Vec::new() }
    }

    pub fn new_random(num_sites: usize, width: i32, height: i32) -> Self {
        let mut sites = Vec::new();
        for _ in 0..num_sites {
            let x = rand::random::<f64>() * width as f64;
            let y = rand::random::<f64>() * height as f64;
            let color = (rand::random::<f64>(), rand::random::<f64>(), rand::random::<f64>());
            let last_point = (Point { x, y }, Point { x, y });
            sites.push(Site { x, y, color, last_point        });
        }
        Voronoi { width, height, directrix: 0.0, sites, active_sites: Vec::new() }
    }

    pub fn draw(&self, width: i32, height: i32, ctx: &cairo::Context) {
        for site in &self.sites {
            site.draw(self.directrix,width, height, ctx);
        }
        // render the text
        ctx.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        ctx.select_font_face(
            "Monospace",
            cairo::FontSlant::Normal,
            cairo::FontWeight::Normal,
        );
        ctx.set_font_size(12.0);
        ctx.set_line_width(1.0);
        ctx.move_to(8.0, 34.0);
        if let Err(_e) = ctx.show_text(format!("Directrix: {}", self.directrix).as_str()) {
            // TODO handle error
        }
        ctx.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        ctx.move_to(0.0, self.directrix);
        ctx.line_to(width as f64, self.directrix);
        ctx.stroke().unwrap();
        self.beachline(ctx);
    }

    fn beachline(&self, ctx: &cairo::Context) {
        let clip = ctx.clip_extents().unwrap();
        // TODO implement beachline
        let mut x = 0.0;
        let mut prev_site = Some(0);
        let mut site_idx: usize = 0;
        let active_sites: Vec<&Site> = self.sites.iter().filter(|s| 
            s.y < self.directrix).collect();
        if active_sites.is_empty() {
            return;
        }
        ctx.set_line_width(0.5);
    
        while x < self.width as f64 {
            let mut max_y = std::f64::NEG_INFINITY;
            for (idx, site) in active_sites.iter().enumerate() {
                let y = 1.0 / (2.0 * (site.y - self.directrix)) * ((x - site.x) * (x - site.x))
                    + ((site.y + self.directrix) / 2.0);
                if y > max_y {
                    max_y = y;
                    site_idx = idx;
                }
            }
            if prev_site.is_none() || prev_site.unwrap() != site_idx {
                ctx.stroke().unwrap();
                prev_site = Some(site_idx);
            }
            x += 0.25;
            let site_color = active_sites[site_idx].color;

            if max_y >= 0.0 && max_y <= clip.3 as f64 {
                println!("x: {}, max_y: {}, site_idx: {}", x, max_y, site_idx);
                ctx.move_to(x as f64, max_y);
                ctx.set_source_rgba(site_color.0, site_color.1, site_color.2, 1.0);
                //ctx.new_path();
                ctx.arc(x, max_y, 0.5, 0.0, 2.0 * std::f64::consts::PI);
            } 
        } 
        if let  Err(_e) = ctx.stroke() {
            println!("Error stroking beachline: {:?}", _e);
        }   

    }
}