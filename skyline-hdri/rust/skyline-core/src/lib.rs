//! skyline-core: 360° skyline plates (terrain + buildings) from any viewpoint, std-only.
//!
//! Pipeline: terrain tiles → Mercator mosaics → azimuthal-equidistant height tiers (+ OSM building
//! footprints rasterised in) → per-column sweep → plate layers (coverage, depth, class, normal,
//! altitude, horizon bands) → PNG / EXR / SVG exports.
//!
//! The algorithm is the same as the JavaScript prototype (`src/engine`) and the GLSL march
//! (`src/gpu/shaders.js`); the three implementations are cross-checked on one synthetic fixture.

pub mod geo;
pub mod png;
pub mod json;
pub mod tiles;
pub mod tiers;
pub mod buildings;
pub mod sweep;
pub mod http;
pub mod scene;
pub mod export;

pub use scene::{prepare_scene, render, BuildingParams, Params, Progress, NoProgress, Scene, Stats};
pub use sweep::Plate;
