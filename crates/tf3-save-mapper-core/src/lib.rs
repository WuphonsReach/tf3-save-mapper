//! Reads a Transport Fever 3 save and draws it as an SVG map.
//!
//! This crate holds the library side: container, header, street edges,
//! terrain, model instances, the strip table's edge ids, the lane table, the
//! filters for loose industry pieces and for an industry's own edges inside it, the facts read from
//! the game install at run time, and the SVG writer (and a flat PNG of the
//! terrain and the network). It has no command-line
//! code and never writes a save. The layout it reads is described in the
//! tf3-save-format notes.

pub mod container;
pub mod contour;
pub mod edges;
pub mod error;
pub mod geometry;
pub mod header;
pub mod inside;
pub mod install;
pub mod instances;
pub mod lanes;
pub mod loose;
mod reader;
pub mod strips;
pub mod svg;
pub mod terrain;
mod zip;

pub use edges::{Edge, Family, read_edges, read_free_ids, retain_live};
pub use error::{Error, Result};
pub use header::{Header, read_header};
pub use inside::{industry_footprints, retain_outside_footprints};
pub use install::{Install, Mods};
pub use instances::{Instance, ModelTable, industry_models, read_instances, read_model_table};
pub use lanes::{read_lane_ids, retain_laned};
pub use loose::retain_near_industries;
pub use strips::{edge_ids, read_strips};
pub use svg::{Layers, SvgOptions, write_png, write_svg};
pub use terrain::{Terrain, read_terrain};
