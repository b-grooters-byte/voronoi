//! Lloyd's relaxation: repeatedly replace each site with its Voronoi
//! cell's centroid and re-run the sweep, which iteratively evens out site
//! spacing. Talks to `Voronoi` only through its public surface —
//! `cell_polygons`, `sites`, `width`/`height`, and
//! `start_sweep`/`advance_to`/`finish_tessellation` — so it has no need
//! to know anything about the beachline or event-queue internals that
//! actually build the tessellation.

use crate::voronoi::{Point, Voronoi};

/// Area-weighted centroid of a simple polygon, via the standard
/// shoelace-based formula. `None` for a degenerate (near-zero-area)
/// polygon.
fn polygon_centroid(poly: &[Point]) -> Option<Point> {
    let n = poly.len();
    if n < 3 {
        return None;
    }
    let mut area2 = 0.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        let p0 = poly[i];
        let p1 = poly[(i + 1) % n];
        let cross = p0.x * p1.y - p1.x * p0.y;
        area2 += cross;
        cx += (p0.x + p1.x) * cross;
        cy += (p0.y + p1.y) * cross;
    }
    if area2.abs() < 1e-9 {
        return None;
    }
    let area = area2 / 2.0;
    Some(Point {
        x: cx / (6.0 * area),
        y: cy / (6.0 * area),
    })
}

/// Runs `passes` iterations of Lloyd's relaxation on `voronoi`: replace
/// each site with its current cell's centroid, then re-run the sweep to
/// completion, repeating. Requires `voronoi` to already have a finished
/// tessellation — the caller gates the UI control on
/// `Voronoi::is_finished`. A site whose cell doesn't form a well-defined
/// polygon (e.g. too few sites overall) is left where it is for that
/// pass.
///
/// Runs synchronously (no animation): at up to a few hundred sites this
/// is fast enough not to need the tokio/animated machinery the Start
/// button uses for the initial sweep.
pub fn relax(voronoi: &mut Voronoi, passes: usize) {
    for _ in 0..passes {
        for (site_idx, poly) in voronoi.cell_polygons() {
            if let Some(centroid) = polygon_centroid(&poly) {
                voronoi.sites[site_idx].x = centroid.x;
                voronoi.sites[site_idx].y = centroid.y;
            }
        }
        voronoi.start_sweep();
        voronoi.advance_to(voronoi.height as f64);
        voronoi.finish_tessellation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voronoi::Site;

    #[test]
    fn polygon_centroid_of_a_square() {
        let square = [
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 0.0, y: 10.0 },
        ];
        let c = polygon_centroid(&square).expect("square has positive area");
        assert!((c.x - 5.0).abs() < 1e-9);
        assert!((c.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn polygon_centroid_rejects_degenerate_polygons() {
        assert!(polygon_centroid(&[Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }]).is_none());
        // Three collinear points: zero area.
        let line = [
            Point { x: 0.0, y: 0.0 },
            Point { x: 5.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
        ];
        assert!(polygon_centroid(&line).is_none());
    }

    /// One relaxation pass should move each site *exactly* to the
    /// centroid of the cell it had immediately beforehand — this checks
    /// `relax`'s own bookkeeping (compute-then-move-then-resweep) in
    /// isolation from whether `cell_polygons` itself is geometrically
    /// correct (covered separately in `voronoi::tests`).
    #[test]
    fn relax_moves_each_site_to_its_pre_relax_cell_centroid() {
        let mut v = Voronoi::new(100, 100);
        v.sites.push(Site {
            x: 30.0,
            y: 20.0,
            color: (0.0, 0.0, 0.0),
        });
        v.sites.push(Site {
            x: 70.0,
            y: 80.0,
            color: (0.0, 0.0, 0.0),
        });
        v.start_sweep();
        v.advance_to(100.0);
        v.finish_tessellation();

        let before = v.cell_polygons();
        let centroid_of = |idx: usize| {
            polygon_centroid(&before.iter().find(|(i, _)| *i == idx).unwrap().1)
                .expect("both cells are well-formed quadrilaterals")
        };
        let expected0 = centroid_of(0);
        let expected1 = centroid_of(1);

        relax(&mut v, 1);

        assert!((v.sites[0].x - expected0.x).abs() < 1e-6);
        assert!((v.sites[0].y - expected0.y).abs() < 1e-6);
        assert!((v.sites[1].x - expected1.x).abs() < 1e-6);
        assert!((v.sites[1].y - expected1.y).abs() < 1e-6);
    }
}
