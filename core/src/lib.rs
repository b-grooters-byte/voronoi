//! Platform-agnostic Voronoi diagram construction (Fortune's algorithm)
//! and Lloyd's relaxation. No rendering-backend dependencies — this crate
//! only produces data (sites, cell edges, cell polygons); each host
//! application (gtk desktop, web/wasm) owns its own renderer over that
//! data.

pub mod lloyd;
pub mod voronoi;

pub use lloyd::relax;
pub use voronoi::*;
