use gtk4::cairo;
use std::cmp;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub const PARABOLA_X_STEP: usize = 5;

/// Upper bound (in either direction, on either axis) for any coordinate
/// handed to Cairo. Breakpoint/parabola math divides by quantities that
/// can get very close to zero near a near-degenerate configuration (e.g.
/// two sites with nearly-equal y), producing values that are finite in
/// `f64` but far beyond Cairo's internal fixed-point coordinate range —
/// which silently breaks all further drawing on that context, not just
/// the one bad segment. `SAFE_COORD` is generously larger than any
/// realistic canvas (so clamped points are still comfortably off-screen,
/// drawing looks unchanged) but well within Cairo's safe range.
const SAFE_COORD: f64 = 1.0e5;

/// Clamps a single coordinate to `[-SAFE_COORD, SAFE_COORD]`. NaN maps to
/// 0.0 (no sane direction to push it); +/-Infinity map to the
/// corresponding bound.
fn clamp_coord(v: f64) -> f64 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(-SAFE_COORD, SAFE_COORD)
    }
}

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
/// * color - RGB color tuple representing the color of the site.
#[derive(Debug, Copy, Clone)]
pub struct Site {
    pub x: f64,
    pub y: f64,
    pub color: (f64, f64, f64),
}

impl Site {
    /// Renders the site marker. The parabola itself is no longer drawn
    /// here — only the portion that's actually part of the beachline
    /// matters, and `Voronoi::beachline` already draws exactly that.
    pub fn draw(&self, _directrix: f64, _width: i32, _height: i32, ctx: &cairo::Context) {
        ctx.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        ctx.set_line_width(1.0);
        ctx.new_path();
        ctx.arc(self.x, self.y, 2.0, 0.0, 2.0 * std::f64::consts::PI);
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
    pub parent: Option<NodeIdx>,
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

    /// Appends a node to the arena and returns its index. Nodes are never
    /// physically removed (a deleted node's slot is set to `None` instead)
    /// so existing `NodeIdx` values remain valid for the lifetime of the tree.
    fn alloc(&mut self, node: BeachNode) -> NodeIdx {
        self.nodes.push(Some(node));
        self.nodes.len() - 1
    }

    fn get(&self, idx: NodeIdx) -> &BeachNode {
        self.nodes[idx]
            .as_ref()
            .expect("beachline node index points at a freed slot")
    }

    fn get_mut(&mut self, idx: NodeIdx) -> &mut BeachNode {
        self.nodes[idx]
            .as_mut()
            .expect("beachline node index points at a freed slot")
    }

    fn parent_of(&self, idx: NodeIdx) -> Option<NodeIdx> {
        match self.get(idx) {
            BeachNode::Arc(a) => a.parent,
            BeachNode::BreakPoint(bp) => bp.parent,
        }
    }

    fn set_parent(&mut self, idx: NodeIdx, parent: Option<NodeIdx>) {
        match self.get_mut(idx) {
            BeachNode::Arc(a) => a.parent = parent,
            BeachNode::BreakPoint(bp) => bp.parent = parent,
        }
    }
}

/// A pending event in the sweep. `Site` events are known up front (one per
/// input site); `Circle` events are discovered as the beachline evolves and
/// predict where/when an arc will shrink to nothing. `Circle` carries an
/// index into `Voronoi::circle_events` rather than the data itself, so that
/// event can be invalidated in place (see `CircleEvent::valid`) without
/// having to remove it from the heap.
#[derive(Debug, Clone, Copy)]
enum EventKind {
    Site(SiteIdx),
    Circle(usize),
}

/// One entry in the event priority queue, ordered by `y` (then `x` to break
/// ties deterministically) so `BinaryHeap<Reverse<HeapEvent>>` pops events in
/// the order the sweep line reaches them.
#[derive(Debug, Clone, Copy)]
struct HeapEvent {
    pub y: f64,
    pub x: f64,
    pub kind: EventKind,
}

impl PartialEq for HeapEvent {
    fn eq(&self, other: &Self) -> bool {
        self.y == other.y && self.x == other.x
    }
}
impl Eq for HeapEvent {}

impl PartialOrd for HeapEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.y
            .partial_cmp(&other.y)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.x.partial_cmp(&other.x).unwrap_or(Ordering::Equal))
    }
}

/// A predicted convergence of three consecutive beachline arcs. `arc_node`
/// is the middle arc that will disappear when the sweep reaches `y` (the
/// bottom of the circumcircle through the three arcs' sites). `valid` is
/// flipped to `false` if the beachline changes in a way that invalidates
/// the prediction before the sweep gets there; such events are skipped
/// when popped rather than removed from the heap.
#[derive(Debug, Clone, Copy)]
struct CircleEvent {
    pub x: f64,
    pub center_y: f64,
    pub arc_node: NodeIdx,
    pub valid: bool,
}

/// A Voronoi cell edge: the boundary between `left_site` and `right_site`'s
/// cells, traced out by one breakpoint as the sweep advances. `start` is
/// fixed the moment the edge is born (either where a new site splits an
/// arc, or where a circle event's vertex closes off one edge and opens the
/// next). `end` is the live tip while the owning breakpoint is still part
/// of the beachline, and becomes fixed — the actual Voronoi vertex — once
/// `done` is set by a circle event.
#[derive(Debug, Clone, Copy)]
struct Edge {
    start: Point,
    end: Point,
    left_site: SiteIdx,
    right_site: SiteIdx,
    done: bool,
}

#[derive(Debug, Clone)]
pub struct Voronoi {
    pub width: i32,
    pub height: i32,
    pub directrix: f64,
    pub sites: Vec<Site>,
    pub active_sites: Vec<Site>,

    beachline: BeachLine,
    circle_events: Vec<CircleEvent>,
    event_queue: BinaryHeap<cmp::Reverse<HeapEvent>>,
    edges: Vec<Edge>,
    /// True once `finish_tessellation` has run the sweep past the visible
    /// canvas and every edge crossing it has reached its true endpoint.
    /// `draw` uses this to stop showing the (now meaningless) sweep line
    /// and transient beachline once the diagram is complete.
    finished: bool,
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
            circle_events: Vec::new(),
            event_queue: BinaryHeap::new(),
            edges: Vec::new(),
            finished: false,
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
            circle_events: Vec::new(),
            event_queue: BinaryHeap::new(),
            edges: Vec::new(),
            finished: false,
        }
    }

    /// Replaces all sites with a fresh random set and resets the sweep state.
    pub fn regenerate(&mut self, count: usize) {
        self.directrix = 0.0;
        self.beachline = BeachLine::new();
        self.circle_events.clear();
        self.event_queue.clear();
        self.edges.clear();
        self.finished = false;
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

    /// Resets the sweep to the top of the canvas and seeds the event queue
    /// with every site as a pending site event, ready for `advance_to` to
    /// process. Call this once when a sweep run starts (the sites
    /// themselves are left untouched — `regenerate` handles picking new
    /// ones).
    pub fn start_sweep(&mut self) {
        self.directrix = 0.0;
        self.beachline = BeachLine::new();
        self.circle_events.clear();
        self.event_queue.clear();
        self.edges.clear();
        self.finished = false;
        for (idx, site) in self.sites.iter().enumerate() {
            self.event_queue.push(cmp::Reverse(HeapEvent {
                y: site.y,
                x: site.x,
                kind: EventKind::Site(idx),
            }));
        }
    }

    /// Advances the sweep to `y`, processing every queued event the sweep
    /// reaches along the way — each handled at its own exact y, so the
    /// beachline is always exactly correct at the moment an event fires —
    /// then settles `directrix` at `y` for display. Called on every tick
    /// of the animated sweep; most ticks process zero events and just move
    /// the directrix.
    pub fn advance_to(&mut self, y: f64) {
        while let Some(cmp::Reverse(next)) = self.event_queue.peek().copied() {
            if next.y > y {
                break;
            }
            self.event_queue.pop();
            self.directrix = next.y;
            match next.kind {
                EventKind::Site(site_idx) => self.handle_site_event(site_idx),
                EventKind::Circle(event_idx) => self.handle_circle_event(event_idx),
            }
        }
        self.directrix = y;
    }

    /// True if any still-growing edge's current tip lies within the
    /// visible canvas rectangle. `finish_tessellation` advances the sweep
    /// until this is false for every open edge — checking the full
    /// rectangle (not just y) matters because a bisector between two
    /// sites with the same x is exactly horizontal: its tip's y never
    /// changes as the sweep advances, only x does, so it only ever exits
    /// through a side, never the bottom.
    fn open_edge_visible(&self) -> bool {
        let Some(root) = self.beachline.root else {
            return false;
        };
        let mut breakpoints = Vec::new();
        self.collect_breakpoints_inorder(root, &mut breakpoints);
        for bp_idx in breakpoints {
            let (half_edge, left_site, right_site) = match self.beachline.get(bp_idx) {
                BeachNode::BreakPoint(bp) => (bp.half_edge, bp.left_site, bp.right_site),
                BeachNode::Arc(_) => {
                    unreachable!("collect_breakpoints_inorder only visits breakpoints")
                }
            };
            let Some(edge_idx) = half_edge else { continue };
            if self.edges[edge_idx].done {
                continue;
            }
            let x = self.breakpoint_x(&self.sites[left_site], &self.sites[right_site]);
            let y = self.parabola_y(&self.sites[left_site], x);
            if x.is_finite()
                && y.is_finite()
                && x >= 0.0
                && x <= self.width as f64
                && y >= 0.0
                && y <= self.height as f64
            {
                return true;
            }
        }
        false
    }

    /// Called once the animated sweep reaches the bottom of the canvas.
    /// Edges on the diagram's outer hull never get a circle event, so
    /// their tip just stops wherever the sweep happened to be when the
    /// animation ended — visually cut short, rather than reaching the
    /// canvas edge. Since a breakpoint always moves along the exact
    /// straight perpendicular-bisector line of its two sites (that's the
    /// literal locus of points equidistant from both), monotonically as
    /// the sweep advances, pushing the directrix further doesn't change
    /// direction — it just continues each open edge along the same line
    /// until it actually leaves the canvas. This does the pushing
    /// (unanimated — it's not meant to be watched), in exponentially
    /// larger jumps, processing whatever events fall due along the way,
    /// until nothing open is left inside the canvas.
    pub fn finish_tessellation(&mut self) {
        if !self.open_edge_visible() {
            self.finished = true;
            return;
        }
        let mut target = self.directrix.max(1.0);
        for _ in 0..60 {
            target *= 2.0;
            self.advance_to(target);
            if !self.open_edge_visible() {
                break;
            }
        }
        self.finished = true;
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

    /// The y-coordinate of `site`'s parabola at `x`, for the current
    /// directrix. Shared by the birth-point calculation below and by edge
    /// rendering; divides by zero if `site.y == self.directrix` exactly
    /// (the single-instant "just inserted" case also guarded elsewhere).
    fn parabola_y(&self, site: &Site, x: f64) -> f64 {
        1.0 / (2.0 * (site.y - self.directrix)) * (x - site.x).powi(2)
            + (site.y + self.directrix) / 2.0
    }

    pub fn find_arc_above(&self, x: f64) -> Option<NodeIdx> {
        let mut node_idx = self.beachline.root?;
        loop {
            match &self.beachline.nodes[node_idx] {
                Some(BeachNode::Arc(_)) => {
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

    /// Nearest ancestor of `node` reached by ascending through a *right*
    /// child edge. That ancestor breakpoint is exactly the one currently
    /// tracking the boundary immediately to `node`'s left — either because
    /// it's `node`'s direct parent (if `node` is its right child), or
    /// because everything between them collapsed from that side. `None`
    /// means `node` is the leftmost node in the tree, i.e. has no left
    /// boundary.
    fn left_boundary_ancestor(&self, node: NodeIdx) -> Option<NodeIdx> {
        let mut cur = node;
        loop {
            let parent = self.beachline.parent_of(cur)?;
            match self.beachline.get(parent) {
                BeachNode::BreakPoint(bp) if bp.right == cur => return Some(parent),
                _ => cur = parent,
            }
        }
    }

    /// Mirror of `left_boundary_ancestor`: nearest ancestor reached via a
    /// *left* child edge, tracking the boundary immediately to `node`'s
    /// right.
    fn right_boundary_ancestor(&self, node: NodeIdx) -> Option<NodeIdx> {
        let mut cur = node;
        loop {
            let parent = self.beachline.parent_of(cur)?;
            match self.beachline.get(parent) {
                BeachNode::BreakPoint(bp) if bp.left == cur => return Some(parent),
                _ => cur = parent,
            }
        }
    }

    /// Returns the beachline arc immediately to the left of `arc_idx`, if
    /// any: found via `left_boundary_ancestor`, then descending into that
    /// breakpoint's left subtree as far right as possible (the rightmost
    /// leaf of "everything to the left" is the nearest one).
    fn prev_arc(&self, arc_idx: NodeIdx) -> Option<NodeIdx> {
        let bp_idx = self.left_boundary_ancestor(arc_idx)?;
        let mut cur = match self.beachline.get(bp_idx) {
            BeachNode::BreakPoint(bp) => bp.left,
            BeachNode::Arc(_) => unreachable!("left_boundary_ancestor always returns a breakpoint"),
        };
        loop {
            match self.beachline.get(cur) {
                BeachNode::Arc(_) => return Some(cur),
                BeachNode::BreakPoint(inner) => cur = inner.right,
            }
        }
    }

    /// Mirror image of `prev_arc`: the beachline arc immediately to the
    /// right of `arc_idx`, if any.
    fn next_arc(&self, arc_idx: NodeIdx) -> Option<NodeIdx> {
        let bp_idx = self.right_boundary_ancestor(arc_idx)?;
        let mut cur = match self.beachline.get(bp_idx) {
            BeachNode::BreakPoint(bp) => bp.right,
            BeachNode::Arc(_) => {
                unreachable!("right_boundary_ancestor always returns a breakpoint")
            }
        };
        loop {
            match self.beachline.get(cur) {
                BeachNode::Arc(_) => return Some(cur),
                BeachNode::BreakPoint(inner) => cur = inner.left,
            }
        }
    }

    /// Drops `arc_idx`'s pending circle event, if it has one. Called
    /// whenever the beachline changes in a way that makes a previously
    /// predicted convergence stale (the arc's neighbors are about to
    /// change, so the old prediction no longer describes the future).
    fn invalidate_circle_event(&mut self, arc_idx: NodeIdx) {
        let event_idx = match self.beachline.get(arc_idx) {
            BeachNode::Arc(a) => a.circle_event,
            BeachNode::BreakPoint(_) => None,
        };
        if let Some(idx) = event_idx {
            self.circle_events[idx].valid = false;
        }
        if let BeachNode::Arc(a) = self.beachline.get_mut(arc_idx) {
            a.circle_event = None;
        }
    }

    /// Starts a new cell edge at `start`, between `left_site` and
    /// `right_site`'s cells, and returns its index. `end` is initialized
    /// to `start` (a zero-length edge) since it will track a live
    /// breakpoint's position until `finish_edge` fixes it.
    fn new_edge(&mut self, start: Point, left_site: SiteIdx, right_site: SiteIdx) -> usize {
        let idx = self.edges.len();
        self.edges.push(Edge {
            start,
            end: start,
            left_site,
            right_site,
            done: false,
        });
        idx
    }

    /// Permanently closes off the edge owned by `bp_idx` at `vertex`. No-op
    /// if `bp_idx` isn't a breakpoint or has no edge yet.
    fn finish_edge(&mut self, bp_idx: NodeIdx, vertex: Point) {
        let edge_idx = match self.beachline.get(bp_idx) {
            BeachNode::BreakPoint(bp) => bp.half_edge,
            BeachNode::Arc(_) => None,
        };
        if let Some(idx) = edge_idx {
            self.edges[idx].end = vertex;
            self.edges[idx].done = true;
        }
    }

    /// A site event: a new site becomes active as the sweep reaches its y.
    /// It always lands directly under exactly one existing arc (or becomes
    /// the very first arc), so handling it means splitting that one arc.
    fn handle_site_event(&mut self, site_idx: SiteIdx) {
        if self.beachline.root.is_none() {
            let root = self.beachline.alloc(BeachNode::Arc(Arc {
                site: site_idx,
                circle_event: None,
                parent: None,
            }));
            self.beachline.root = Some(root);
            return;
        }

        let arc_idx = self
            .find_arc_above(self.sites[site_idx].x)
            .expect("a non-empty beachline has an arc above every x");
        let arc_site = match self.beachline.get(arc_idx) {
            BeachNode::Arc(a) => a.site,
            BeachNode::BreakPoint(_) => unreachable!("find_arc_above always returns an Arc"),
        };

        // The split arc's old neighbors are about to change, so any circle
        // event predicted for it no longer applies.
        self.invalidate_circle_event(arc_idx);
        let old_parent = self.beachline.parent_of(arc_idx);

        // A single arc under `arc_site` becomes three arcs separated by two
        // new breakpoints, arranged as:
        //
        //         bp_left (arc_site | site_idx)
        //        /                            \
        //   arc(arc_site)              bp_right (site_idx | arc_site)
        //                                /                          \
        //                          arc(site_idx)               arc(arc_site)
        //
        // The new site's arc starts with zero width right at the old arc's
        // parabola — both breakpoints coincide at that point until the
        // sweep moves further and they separate.
        let left_copy = self.beachline.alloc(BeachNode::Arc(Arc {
            site: arc_site,
            circle_event: None,
            parent: None,
        }));
        let mid_arc = self.beachline.alloc(BeachNode::Arc(Arc {
            site: site_idx,
            circle_event: None,
            parent: None,
        }));
        let right_copy = self.beachline.alloc(BeachNode::Arc(Arc {
            site: arc_site,
            circle_event: None,
            parent: None,
        }));

        // Both new breakpoints are born at the same point: directly above
        // the new site, on arc_site's parabola. From there they diverge in
        // opposite directions, each tracing one edge of the same bisector
        // line between arc_site and site_idx.
        let new_site_x = self.sites[site_idx].x;
        let birth = Point {
            x: new_site_x,
            y: self.parabola_y(&self.sites[arc_site], new_site_x),
        };
        let left_edge = self.new_edge(birth, arc_site, site_idx);
        let right_edge = self.new_edge(birth, site_idx, arc_site);

        let bp_right = self.beachline.alloc(BeachNode::BreakPoint(InternalNode {
            parent: None,
            left_site: site_idx,
            right_site: arc_site,
            left: mid_arc,
            right: right_copy,
            half_edge: Some(right_edge),
        }));
        let bp_left = self.beachline.alloc(BeachNode::BreakPoint(InternalNode {
            parent: old_parent,
            left_site: arc_site,
            right_site: site_idx,
            left: left_copy,
            right: bp_right,
            half_edge: Some(left_edge),
        }));
        self.beachline.set_parent(left_copy, Some(bp_left));
        self.beachline.set_parent(bp_right, Some(bp_left));
        self.beachline.set_parent(mid_arc, Some(bp_right));
        self.beachline.set_parent(right_copy, Some(bp_right));

        // Splice bp_left into the tree where the old arc used to be.
        match old_parent {
            None => self.beachline.root = Some(bp_left),
            Some(parent_idx) => {
                if let BeachNode::BreakPoint(p) = self.beachline.get_mut(parent_idx) {
                    if p.left == arc_idx {
                        p.left = bp_left;
                    } else {
                        p.right = bp_left;
                    }
                }
            }
        }
        self.beachline.nodes[arc_idx] = None;

        // Two new consecutive-arc triples now exist, one on each side of
        // the freshly inserted arc; either may converge to a future vertex.
        if let Some(left_neighbor) = self.prev_arc(left_copy) {
            self.check_circle_event(left_neighbor, left_copy, mid_arc);
        }
        if let Some(right_neighbor) = self.next_arc(right_copy) {
            self.check_circle_event(mid_arc, right_copy, right_neighbor);
        }
    }

    /// Tests whether three consecutive beachline arcs (`left`, `mid`,
    /// `right`, in left-to-right order) will converge to a single point in
    /// the future, and if so records the predicted circle event on `mid`.
    ///
    /// The three sites' circumcircle always exists mathematically (unless
    /// they're collinear), but that alone doesn't mean the breakpoints are
    /// heading toward each other — they might be spreading apart, in which
    /// case `mid`'s arc never vanishes and there is no real event. `temp`
    /// below is a turn-direction test (cross product of `b-a` and `c-a`)
    /// that distinguishes the two cases in this canvas's y-down coordinate
    /// system: verified numerically against `breakpoint_x` — a diverging
    /// triple gives `temp < 0`, a converging one gives `temp > 0`.
    fn check_circle_event(&mut self, left: NodeIdx, mid: NodeIdx, right: NodeIdx) {
        let left_site = match self.beachline.get(left) {
            BeachNode::Arc(a) => a.site,
            BeachNode::BreakPoint(_) => unreachable!("neighbor lookups always return arcs"),
        };
        let mid_site = match self.beachline.get(mid) {
            BeachNode::Arc(a) => a.site,
            BeachNode::BreakPoint(_) => unreachable!("neighbor lookups always return arcs"),
        };
        let right_site = match self.beachline.get(right) {
            BeachNode::Arc(a) => a.site,
            BeachNode::BreakPoint(_) => unreachable!("neighbor lookups always return arcs"),
        };

        let a = self.sites[left_site];
        let b = self.sites[mid_site];
        let c = self.sites[right_site];

        let temp = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
        if temp <= 0.0 {
            return; // diverging (or degenerate, e.g. left/right are the same site)
        }

        // Circumcenter of a, b, c via the standard determinant formula.
        let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
        if d.abs() < 1e-10 {
            return; // collinear sites: no well-defined circumcircle
        }
        let a2 = a.x * a.x + a.y * a.y;
        let b2 = b.x * b.x + b.y * b.y;
        let c2 = c.x * c.x + c.y * c.y;
        let center_x = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
        let center_y = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
        let radius = ((center_x - a.x).powi(2) + (center_y - a.y).powi(2)).sqrt();

        // The sweep moves toward increasing y, so it reaches the *bottom*
        // of the circumcircle last: that's the moment all three arcs'
        // breakpoints coincide at (center_x, center_y), which is when the
        // event actually fires.
        let event_y = center_y + radius;
        if event_y < self.directrix {
            return; // the convergence point is already behind the sweep
        }

        let event_idx = self.circle_events.len();
        self.circle_events.push(CircleEvent {
            x: center_x,
            center_y,
            arc_node: mid,
            valid: true,
        });
        self.event_queue.push(cmp::Reverse(HeapEvent {
            y: event_y,
            x: center_x,
            kind: EventKind::Circle(event_idx),
        }));
        if let BeachNode::Arc(arc) = self.beachline.get_mut(mid) {
            arc.circle_event = Some(event_idx);
        }
    }

    /// A circle event: the arc predicted by `check_circle_event` has
    /// shrunk to nothing and must be removed from the beachline, merging
    /// its two former neighbors into direct contact.
    fn handle_circle_event(&mut self, event_idx: usize) {
        if !self.circle_events[event_idx].valid {
            return; // a beachline change since this was queued made it stale
        }
        let mid = self.circle_events[event_idx].arc_node;

        let left_arc = self
            .prev_arc(mid)
            .expect("a queued circle event's arc always has a left neighbor");
        let right_arc = self
            .next_arc(mid)
            .expect("a queued circle event's arc always has a right neighbor");

        // The neighbors' own predicted circle events (if any) were computed
        // against a beachline shape that's about to change underneath them.
        self.invalidate_circle_event(left_arc);
        self.invalidate_circle_event(right_arc);

        let left_site = match self.beachline.get(left_arc) {
            BeachNode::Arc(a) => a.site,
            BeachNode::BreakPoint(_) => unreachable!("prev_arc always returns an arc"),
        };
        let right_site = match self.beachline.get(right_arc) {
            BeachNode::Arc(a) => a.site,
            BeachNode::BreakPoint(_) => unreachable!("next_arc always returns an arc"),
        };

        // Exactly two breakpoints touch `mid`: one tracks (left_arc | mid),
        // the other (mid | right_arc). One of the two is mid's direct
        // parent — that one is spliced out. The other survives and is
        // repurposed to track the new (left_arc | right_arc) boundary,
        // since mid no longer separates them.
        let left_bp = self
            .left_boundary_ancestor(mid)
            .expect("mid has a left neighbor, so a left boundary breakpoint exists");
        let right_bp = self
            .right_boundary_ancestor(mid)
            .expect("mid has a right neighbor, so a right boundary breakpoint exists");
        let mid_parent = self
            .beachline
            .parent_of(mid)
            .expect("mid has neighbors, so it cannot be the sole root");
        let (surviving_bp, doomed_bp) = if mid_parent == left_bp {
            (right_bp, left_bp)
        } else {
            (left_bp, right_bp)
        };

        // The vertex where mid's arc vanishes — already computed by
        // check_circle_event as this circumcircle's center. Both edges
        // touching mid (left_bp's and right_bp's) end here.
        let vertex = Point {
            x: self.circle_events[event_idx].x,
            y: self.circle_events[event_idx].center_y,
        };
        self.finish_edge(left_bp, vertex);
        self.finish_edge(right_bp, vertex);

        // Splice `mid` and its direct parent (`doomed_bp`) out of the
        // tree: mid's sibling takes doomed_bp's place in the grandparent.
        let sibling = match self.beachline.get(doomed_bp) {
            BeachNode::BreakPoint(bp) => {
                if bp.left == mid {
                    bp.right
                } else {
                    bp.left
                }
            }
            BeachNode::Arc(_) => unreachable!("doomed_bp is mid's parent breakpoint"),
        };
        let grandparent = self.beachline.parent_of(doomed_bp);
        self.beachline.set_parent(sibling, grandparent);
        match grandparent {
            None => self.beachline.root = Some(sibling),
            Some(g) => {
                if let BeachNode::BreakPoint(gp) = self.beachline.get_mut(g) {
                    if gp.left == doomed_bp {
                        gp.left = sibling;
                    } else {
                        gp.right = sibling;
                    }
                }
            }
        }

        // The surviving breakpoint now tracks a different boundary — the
        // one between left_arc and right_arc directly — so it starts a
        // fresh edge at the vertex we just closed the old ones off at.
        let new_edge = self.new_edge(vertex, left_site, right_site);
        if let BeachNode::BreakPoint(bp) = self.beachline.get_mut(surviving_bp) {
            bp.left_site = left_site;
            bp.right_site = right_site;
            bp.half_edge = Some(new_edge);
        }

        self.beachline.nodes[mid] = None;
        self.beachline.nodes[doomed_bp] = None;

        // The merge creates two new consecutive-arc triples, each a
        // candidate for its own future circle event.
        if let Some(farther_left) = self.prev_arc(left_arc) {
            self.check_circle_event(farther_left, left_arc, right_arc);
        }
        if let Some(farther_right) = self.next_arc(right_arc) {
            self.check_circle_event(left_arc, right_arc, farther_right);
        }
    }

    pub fn draw(&self, width: i32, height: i32, ctx: &cairo::Context) {
        for site in &self.sites {
            site.draw(self.directrix, width, height, ctx);
        }
        // The sweep line, directrix readout, and transient beachline only
        // mean something mid-sweep; once finish_tessellation has run,
        // self.directrix is some large off-canvas value used purely for
        // edge math, not anything worth showing.
        if !self.finished {
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
        self.draw_edges(ctx);
    }

    /// Collects the beachline's breakpoints in left-to-right order (an
    /// in-order traversal; mirrors `collect_arcs_inorder` but visits the
    /// internal nodes instead of the leaves).
    fn collect_breakpoints_inorder(&self, node: NodeIdx, out: &mut Vec<NodeIdx>) {
        if let BeachNode::BreakPoint(bp) = self.beachline.get(node) {
            self.collect_breakpoints_inorder(bp.left, out);
            out.push(node);
            self.collect_breakpoints_inorder(bp.right, out);
        }
    }

    /// Draws every cell edge built so far: finished edges as a fixed
    /// segment, and each still-growing edge from its birth point out to
    /// its owning breakpoint's current (live) position.
    fn draw_edges(&self, ctx: &cairo::Context) {
        ctx.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        ctx.set_line_width(1.0);
        ctx.new_path();
        for edge in &self.edges {
            if edge.done {
                // A near-collinear triple can produce a circumcenter far
                // enough away to overflow Cairo's usable coordinate range
                // even though it's a perfectly finite f64.
                ctx.move_to(clamp_coord(edge.start.x), clamp_coord(edge.start.y));
                ctx.line_to(clamp_coord(edge.end.x), clamp_coord(edge.end.y));
            }
        }
        if let Some(root) = self.beachline.root {
            let mut breakpoints = Vec::new();
            self.collect_breakpoints_inorder(root, &mut breakpoints);
            for bp_idx in breakpoints {
                let (half_edge, left_site, right_site) = match self.beachline.get(bp_idx) {
                    BeachNode::BreakPoint(bp) => (bp.half_edge, bp.left_site, bp.right_site),
                    BeachNode::Arc(_) => {
                        unreachable!("collect_breakpoints_inorder only visits breakpoints")
                    }
                };
                let Some(edge_idx) = half_edge else { continue };
                let edge = &self.edges[edge_idx];
                if edge.done {
                    continue;
                }
                let x = self.breakpoint_x(&self.sites[left_site], &self.sites[right_site]);
                let y = self.parabola_y(&self.sites[left_site], x);
                if x.is_nan() || y.is_nan() {
                    continue; // same degenerate (directrix == site.y) case beachline() guards against
                }
                ctx.move_to(clamp_coord(edge.start.x), clamp_coord(edge.start.y));
                ctx.line_to(clamp_coord(x), clamp_coord(y));
            }
        }
        if let Err(_e) = ctx.stroke() {
            println!("Error stroking cell edges: {:?}", _e);
        }
    }

    /// Collects the beachline's arcs in left-to-right order (an in-order
    /// traversal, since arcs are the tree's leaves in exactly that order).
    fn collect_arcs_inorder(&self, node: NodeIdx, out: &mut Vec<SiteIdx>) {
        match self.beachline.get(node) {
            BeachNode::Arc(a) => out.push(a.site),
            BeachNode::BreakPoint(bp) => {
                self.collect_arcs_inorder(bp.left, out);
                self.collect_arcs_inorder(bp.right, out);
            }
        }
    }

    /// Returns each current beachline arc's site along with the x-range it
    /// actually occupies, clipped to `[x_min, x_max]`. The range between
    /// two neighboring arcs is exactly `breakpoint_x` between their sites
    fn beachline_arcs(&self, x_min: f64, x_max: f64) -> Vec<(SiteIdx, f64, f64)> {
        let mut arcs = Vec::new();
        if let Some(root) = self.beachline.root {
            self.collect_arcs_inorder(root, &mut arcs);
        }
        let mut result = Vec::with_capacity(arcs.len());
        let mut start_x = x_min;
        for i in 0..arcs.len() {
            let end_x = if i + 1 < arcs.len() {
                self.breakpoint_x(&self.sites[arcs[i]], &self.sites[arcs[i + 1]])
            } else {
                x_max
            };
            result.push((arcs[i], start_x, end_x));
            start_x = end_x;
        }
        result
    }

    fn beachline(&self, ctx: &cairo::Context) {
        let clip = ctx.clip_extents().unwrap();
        ctx.set_line_width(1.0);
        for (site_idx, x_start, x_end) in self.beachline_arcs(0.0, self.width as f64) {
            let site = &self.sites[site_idx];
            // Right at the moment a site is inserted its arc has zero
            // width (directrix == site.y), which would divide by zero
            // below; skip drawing it for that single instant.
            if (site.y - self.directrix).abs() < 1e-9 || x_end - x_start < 1e-6 {
                continue;
            }
            ctx.set_source_rgba(site.color.0, site.color.1, site.color.2, 1.0);
            ctx.new_path();
            let mut started = false;
            let mut x = x_start.max(0.0);
            let end = x_end.min(clip.2);
            while x <= end {
                let y = clamp_coord(
                    1.0 / (2.0 * (site.y - self.directrix)) * ((x - site.x) * (x - site.x))
                        + ((site.y + self.directrix) / 2.0),
                );
                if started {
                    ctx.line_to(x, y);
                } else {
                    ctx.move_to(x, y);
                    started = true;
                }
                x += PARABOLA_X_STEP as f64;
            }
            if let Err(_e) = ctx.stroke() {
                println!("Error stroking beachline: {:?}", _e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc_sites(v: &Voronoi) -> Vec<SiteIdx> {
        let mut out = Vec::new();
        if let Some(root) = v.beachline.root {
            v.collect_arcs_inorder(root, &mut out);
        }
        out
    }

    /// Three sites, hand-picked with distinct y-coordinates (so no two
    /// site events tie exactly — see the note in `check_circle_event`'s
    /// caller about sites needing "general position"), that produce one
    /// genuine circle event. The expected arc sequences and the predicted
    /// circle-event y (~89.2) were derived by hand against this exact
    /// `check_circle_event` formula before writing the test.
    #[test]
    fn beachline_tracks_site_and_circle_events() {
        let mut v = Voronoi::new(100, 150);
        v.sites.push(Site {
            x: 50.0,
            y: 5.0,
            color: (0.0, 0.0, 0.0),
        }); // 0
        v.sites.push(Site {
            x: 10.0,
            y: 30.0,
            color: (0.0, 0.0, 0.0),
        }); // 1
        v.sites.push(Site {
            x: 90.0,
            y: 40.0,
            color: (0.0, 0.0, 0.0),
        }); // 2
        v.start_sweep();

        v.advance_to(5.0);
        assert_eq!(arc_sites(&v), vec![0]);

        v.advance_to(30.0);
        assert_eq!(arc_sites(&v), vec![0, 1, 0]);

        v.advance_to(40.0);
        assert_eq!(arc_sites(&v), vec![0, 1, 0, 2, 0]);

        // Site 0's middle arc (between sites 1 and 2) is squeezed out by a
        // circle event predicted at y ~= 89.2.
        v.advance_to(100.0);
        assert_eq!(arc_sites(&v), vec![0, 1, 2, 0]);

        // A Voronoi vertex has degree 3: two edges finish there (the ones
        // that bordered the vanished arc, i.e. site pairs {1,0} and {0,2})
        // and one begins (the new {1,2} boundary, still growing). The
        // vertex position is this triple's circumcenter, hand-derived
        // alongside the event-y check above.
        let done: Vec<&Edge> = v.edges.iter().filter(|e| e.done).collect();
        assert_eq!(done.len(), 2);
        let expected_x = 48.4896;
        let expected_y = 47.0833;
        for edge in &done {
            assert!((edge.end.x - expected_x).abs() < 1e-3);
            assert!((edge.end.y - expected_y).abs() < 1e-3);
        }
        let mut site_pairs: Vec<(SiteIdx, SiteIdx)> =
            done.iter().map(|e| (e.left_site, e.right_site)).collect();
        site_pairs.sort();
        assert_eq!(site_pairs, vec![(0, 2), (1, 0)]);

        // The new {1,2} edge born at that vertex is still growing.
        let growing: Vec<&Edge> = v
            .edges
            .iter()
            .filter(|e| !e.done && e.left_site == 1 && e.right_site == 2)
            .collect();
        assert_eq!(growing.len(), 1);
        assert!((growing[0].start.x - expected_x).abs() < 1e-3);
        assert!((growing[0].start.y - expected_y).abs() < 1e-3);
    }

    /// After the animated sweep reaches the bottom of a 150-tall canvas,
    /// the outer-hull edges (sites 0|1, 2|0, and the new 1|2 edge) are
    /// still growing with their tip inside the canvas. `finish_tessellation`
    /// should push the sweep further until none of them are.
    #[test]
    fn finish_tessellation_closes_off_open_edges() {
        let mut v = Voronoi::new(100, 150);
        v.sites.push(Site {
            x: 50.0,
            y: 5.0,
            color: (0.0, 0.0, 0.0),
        });
        v.sites.push(Site {
            x: 10.0,
            y: 30.0,
            color: (0.0, 0.0, 0.0),
        });
        v.sites.push(Site {
            x: 90.0,
            y: 40.0,
            color: (0.0, 0.0, 0.0),
        });
        v.start_sweep();
        v.advance_to(150.0);
        assert!(v.open_edge_visible());
        assert!(!v.finished);

        v.finish_tessellation();
        assert!(!v.open_edge_visible());
        assert!(v.finished);
    }
}
