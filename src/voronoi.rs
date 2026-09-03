use gtk4::cairo;
use std::cmp;

pub const PARABOLA_X_STEP: usize = 5;

type NodeIdx = usize;
type SiteIdx = usize;

#[derive(Debug, Copy, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Site is a simple truct that represents the current state of a simplified
/// Voronoi site. The fields are :
/// * x - X Position
/// * y - Y Position
#[derive(Debug, Copy, Clone)]
pub struct Site {
    pub x: f64,
    pub y: f64,
    pub color: (f64, f64, f64),
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
            let y = 1.0 / (2.0 * (self.y - directrix))
                * ((x as f64 - self.x) * (x as f64 - self.x))
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

/// Arc represents an arc in the beachline. It contains the index of the site
/// that creates the arc, as well as an optional index of a circle event that
/// may occur when the arc disappears from the beachline. The circle event is
/// used to keep track of potential events that may occur during the algorithm,
/// allowing for efficient updates to the beachline as the algorithm progresses.
#[derive(Debug, Clone)]
struct Arc {
    pub site: usize,
    pub circle_event: Option<usize>,
}

/// InternalNode represents a breakpoint in the beachline. It contains the
/// indices of the left and right sites that create the breakpoint, as well as
/// the indices of the left and right child nodes in the beachline binary tree.
/// The half_edge field is an optional index that points to the half-edge in the
/// Voronoi diagram that corresponds to this breakpoint.
///
/// ## Fields:
/// * left_site: usize - The index of the site that creates the left side of the breakpoint.
/// * right_site: usize - The index of the site that creates the right side of the breakpoint.
/// * left: NodeIdx - The index of the left child node in the beachline binary tree.
/// * right: NodeIdx - The index of the right child node in the beachline binary tree.
/// * half_edge: Option<usize> - An optional index that points to the half-edge
/// in the Voronoi diagram that corresponds to this breakpoint. This is used to
/// keep track of the edges in the Voronoi diagram as they are created and updated
/// during the algorithm.
#[derive(Debug, Clone)]
struct InternalNode {
    pub parent: Option<NodeIdx>,
    pub left_site: SiteIdx,
    pub right_site: SiteIdx,
    pub left: NodeIdx,
    pub right: NodeIdx,
    pub half_edge: Option<usize>,
}

#[derive(Debug, Clone)]
enum BeachNode {
    Arc(Arc),
    BreakPoint(InternalNode),
}

#[derive(Debug, Clone)]
struct BeachLine {
    nodes: Vec<Option<BeachNode>>,
    root: Option<usize>,
}

impl BeachLine {
    pub fn new() -> Self {
        BeachLine {
            nodes: Vec::new(),
            root: None,
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

    pub beachline: BeachLine,
}

impl Voronoi {
    pub fn new(width: i32, height: i32) -> Self {
        Voronoi {
            width,
            height,
            directrix: 0.0,
            sites: Vec::new(),
            active_sites: Vec::new(),
            beachline: BeachLine::new(),
        }
    }

    pub fn new_random(num_sites: usize, width: i32, height: i32) -> Self {
        let mut sites = Vec::new();
        for _ in 0..num_sites {
            let x = rand::random::<f64>() * width as f64;
            let y = rand::random::<f64>() * height as f64;
            let color = (
                rand::random::<f64>(),
                rand::random::<f64>(),
                rand::random::<f64>(),
            );
            sites.push(Site { x, y, color });
        }
        Voronoi {
            width,
            height,
            directrix: 0.0,
            sites,
            active_sites: Vec::new(),
            beachline: BeachLine::new(),
        }
    }

    /// Replaces all sites with a fresh random set and resets the sweep state.
    pub fn regenerate(&mut self, count: usize) {
        self.directrix = 0.0;
        self.beachline = BeachLine::new();
        self.sites.clear();
        for _ in 0..count {
            let x = rand::random::<f64>() * self.width as f64;
            let y = rand::random::<f64>() * self.height as f64;
            let color = (
                rand::random::<f64>(),
                rand::random::<f64>(),
                rand::random::<f64>(),
            );
            self.sites.push(Site { x, y, color });
        }
    }

    /// Adopts a new panel size, resetting the sweep and recalculating the
    /// random sites so they stay within the new bounds.
    pub fn resize(&mut self, width: i32, height: i32) {
        self.width = width;
        self.height = height;
        self.regenerate(self.sites.len());
    }

    /// Calculates the x coordinate of the breakpoint between two sites on the beachline
    /// given the current position of the directrix. This is done by solving the
    /// quadratic equation that arises from the definition of the parabolas that form the
    /// beachline. The function takes into account the special case where both sites have
    /// the same y coordinate, which would cause a division by zero in the quadratic formula.
    fn breakpoint_x(&self, left_site: &Site, right_site: &Site) -> f64 {
        // guard against both site having the same y coordinate, which would cause a
        // division by zero in the quadratic formula
        if (left_site.y - right_site.y).abs() < 1e-10 {
            return (left_site.x + right_site.x) / 2.0;
        }
        // calculate the coefficients of the quadratic equation for the breakpoint
        let p = 1.0 / (2.0 * (left_site.y - self.directrix));
        let q = 1.0 / (2.0 * (right_site.y - self.directrix));

        let a = p - q;
        let b = -2.0 * (left_site.x * p - right_site.x * q);
        let c =
            p * left_site.x.powi(2) - q * right_site.x.powi(2) + (left_site.y - right_site.y) / 2.0;

        let disc = b * b - 4.0 * a * c;
        let sqrt_disc = disc.max(0.0).sqrt();
        let x1 = (-b + sqrt_disc) / (2.0 * a);
        let x2 = (-b - sqrt_disc) / (2.0 * a);
        if left_site.y < right_site.y {
            x1.min(x2)
        } else {
            x1.max(x2)
        }
    }

    pub fn find_arc_above(&self, x: f64) -> Option<NodeIdx> {
        let mut node_idx = self.beachline.root?;
        loop {
            match &self.beachline.nodes[node_idx] {
                Some(BeachNode::Arc(arc)) => {
                    return Some(node_idx);
                }
                Some(BeachNode::BreakPoint(bp)) => {
                    let left_site = &self.sites[bp.left_site];
                    let right_site = &self.sites[bp.right_site];
                    let breakpoint_x = self.breakpoint_x(left_site, right_site);
                    if x < breakpoint_x {
                        node_idx = bp.left;
                    } else {
                        node_idx = bp.right;
                    }
                }
                None => {
                    return None; // This should not happen if the beachline is properly maintained
                }
            }
        }
    }

    pub fn draw(&self, width: i32, height: i32, ctx: &cairo::Context) {
        for site in &self.sites {
            site.draw(self.directrix, width, height, ctx);
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
        let active_sites: Vec<&Site> = self.sites.iter().filter(|s| s.y < self.directrix).collect();
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
                ctx.move_to(x as f64, max_y);
                ctx.set_source_rgba(site_color.0, site_color.1, site_color.2, 1.0);
                //ctx.new_path();
                ctx.arc(x, max_y, 0.5, 0.0, 2.0 * std::f64::consts::PI);
            }
        }
        if let Err(_e) = ctx.stroke() {
            println!("Error stroking beachline: {:?}", _e);
        }
    }
}
