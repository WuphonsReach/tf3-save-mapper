//! Reads a Transport Fever 3 save and draws it as an SVG map.
//!
//! This crate holds the library side: container, header, street edges,
//! terrain, model instances, the strip table's edge ids, the lane table, the
//! filters for loose industry pieces and for the industries' sidings, the
//! edges in tunnels and on bridges, the
//! facts read from the game install at run time, and the SVG writer (and a
//! flat PNG of the terrain and the network). It has no command-line code and
//! never writes a save. The layout it reads is described in the
//! tf3-save-format notes.

pub mod container;
pub mod contour;
pub mod edges;
pub mod error;
pub mod geometry;
pub mod header;
pub mod industry_records;
pub mod install;
pub mod instances;
pub mod lanes;
pub mod loose;
mod reader;
pub mod sidings;
pub mod strips;
pub mod structures;
pub mod svg;
pub mod terrain;
mod zip;

pub use edges::{Edge, Family, read_edges, read_free_ids, retain_live};
pub use error::{Error, Result};
pub use header::{Header, read_header};
pub use industry_records::{field_slots, retain_standing_industries};
pub use install::{Install, Mods};
pub use instances::{Instance, ModelTable, industry_models, read_instances, read_model_table};
pub use lanes::{read_lane_ids, retain_laned};
pub use loose::retain_near_industries;
pub use sidings::retain_without_sidings;
pub use strips::{edge_ids, read_strips};
pub use structures::{Structure, find_structures};
pub use svg::{Layers, SvgOptions, write_png, write_svg};
pub use terrain::{Terrain, read_terrain};
