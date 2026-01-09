//! Procedural River Network Generator Library
//!
//! This library provides tools for generating realistic river networks
//! using procedural terrain generation and hydrology simulation.

pub mod terrain;
pub mod hydrology;
pub mod renderer;

pub use terrain::{Terrain, TerrainConfig};
pub use hydrology::{HydrologyData, RiverSegment, FlowDirection};
