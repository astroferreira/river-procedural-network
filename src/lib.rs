//! Procedural River Network Generator Library
//!
//! This library provides tools for generating realistic river networks
//! using procedural terrain generation and hydrology simulation.
//!
//! Based on Nick McDonald's procedural hydrology:
//! https://nickmcd.me/2020/04/15/procedural-hydrology/

pub mod terrain;
pub mod hydrology;
pub mod renderer;
pub mod particle;

pub use terrain::{Terrain, TerrainConfig};
pub use hydrology::{HydrologyData, RiverSegment, FlowDirection};
pub use particle::{ErosionParams, ErosionResult};
