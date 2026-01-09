//! Particle-based hydraulic erosion simulation
//! Based on Nick McDonald's procedural hydrology: https://nickmcd.me/2020/04/15/procedural-hydrology/
//!
//! This module implements a particle-based water simulation where droplets:
//! 1. Spawn at random positions on the terrain
//! 2. Move downhill following gravity and momentum
//! 3. Erode terrain and carry sediment
//! 4. Deposit sediment when slowing down
//! 5. Evaporate over time
//!
//! The cumulative effect creates realistic river networks through erosion.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

/// Simulation parameters for particle-based erosion
#[derive(Clone)]
pub struct ErosionParams {
    /// Number of erosion iterations (particles to simulate)
    pub iterations: usize,
    /// Maximum lifetime of a particle in steps
    pub max_age: usize,
    /// Initial water volume per particle
    pub initial_volume: f32,
    /// Minimum volume before particle dies
    pub min_volume: f32,
    /// Evaporation rate per step
    pub evaporation_rate: f32,
    /// Rate at which particle picks up sediment
    pub erosion_rate: f32,
    /// Rate at which sediment is deposited
    pub deposition_rate: f32,
    /// Inertia factor (0 = pure gradient descent, 1 = pure momentum)
    pub inertia: f32,
    /// Gravity strength
    pub gravity: f32,
    /// Friction factor (how much velocity is retained)
    pub friction: f32,
    /// Minimum slope for erosion to occur
    pub min_slope: f32,
    /// Sediment capacity multiplier
    pub capacity_multiplier: f32,
    /// Momentum transfer rate from discharge map
    pub momentum_transfer: f32,
}

impl Default for ErosionParams {
    fn default() -> Self {
        Self {
            iterations: 100_000,
            max_age: 500,
            initial_volume: 1.0,
            min_volume: 0.01,
            evaporation_rate: 0.001,
            erosion_rate: 0.3,
            deposition_rate: 0.3,
            inertia: 0.05,
            gravity: 4.0,
            friction: 0.1,
            min_slope: 0.0001,
            capacity_multiplier: 8.0,
            momentum_transfer: 1.0,
        }
    }
}

/// A water droplet/particle for erosion simulation
#[derive(Clone, Debug)]
pub struct Drop {
    pub pos: (f32, f32),
    pub vel: (f32, f32),
    pub volume: f32,
    pub sediment: f32,
    pub age: usize,
}

impl Drop {
    pub fn new(x: f32, y: f32, initial_volume: f32) -> Self {
        Self {
            pos: (x, y),
            vel: (0.0, 0.0),
            volume: initial_volume,
            sediment: 0.0,
            age: 0,
        }
    }
}

/// Result of the particle-based erosion simulation
pub struct ErosionResult {
    /// Modified heightmap after erosion
    pub heightmap: Vec<f32>,
    /// Discharge map (accumulated water flow)
    pub discharge: Vec<f32>,
    /// Momentum X component map
    pub momentum_x: Vec<f32>,
    /// Momentum Y component map
    pub momentum_y: Vec<f32>,
    /// Track map (raw discharge before smoothing)
    pub track: Vec<f32>,
}

/// Run the particle-based erosion simulation
pub fn simulate_erosion(
    heightmap: &[f32],
    width: usize,
    height: usize,
    params: &ErosionParams,
    seed: u32,
) -> ErosionResult {
    let mut rng = StdRng::seed_from_u64(seed as u64);

    // Clone heightmap for modification
    let mut heightmap = heightmap.to_vec();

    // Initialize tracking maps
    let mut discharge = vec![0.0f32; width * height];
    let mut discharge_track = vec![0.0f32; width * height];
    let mut momentum_x = vec![0.0f32; width * height];
    let mut momentum_y = vec![0.0f32; width * height];
    let mut momentum_x_track = vec![0.0f32; width * height];
    let mut momentum_y_track = vec![0.0f32; width * height];

    // Smoothing rate for discharge/momentum maps
    let smooth_rate = 0.05;

    // Run erosion iterations
    for i in 0..params.iterations {
        // Spawn particle at random position (with highland bias)
        let x = rng.gen_range(1.0..(width - 1) as f32);
        let y = rng.gen_range(1.0..(height - 1) as f32);

        let mut drop = Drop::new(x, y, params.initial_volume);

        // Simulate particle until it dies
        while drop.age < params.max_age && drop.volume > params.min_volume {
            let success = descend(
                &mut drop,
                &mut heightmap,
                &mut discharge_track,
                &mut momentum_x_track,
                &mut momentum_y_track,
                &discharge,
                &momentum_x,
                &momentum_y,
                width,
                height,
                params,
            );

            if !success {
                break;
            }

            drop.age += 1;
        }

        // Deposit remaining sediment when particle dies
        if drop.sediment > 0.0 {
            let idx = get_index(drop.pos.0 as usize, drop.pos.1 as usize, width, height);
            if let Some(i) = idx {
                heightmap[i] += drop.sediment;
            }
        }

        // Periodically smooth the tracking maps into the main maps
        if i % 1000 == 0 {
            for j in 0..(width * height) {
                discharge[j] = discharge[j] * (1.0 - smooth_rate) + discharge_track[j] * smooth_rate;
                momentum_x[j] = momentum_x[j] * (1.0 - smooth_rate) + momentum_x_track[j] * smooth_rate;
                momentum_y[j] = momentum_y[j] * (1.0 - smooth_rate) + momentum_y_track[j] * smooth_rate;

                // Decay the tracking maps
                discharge_track[j] *= 0.99;
                momentum_x_track[j] *= 0.99;
                momentum_y_track[j] *= 0.99;
            }
        }
    }

    // Final smoothing
    for j in 0..(width * height) {
        discharge[j] = discharge[j] * (1.0 - smooth_rate) + discharge_track[j] * smooth_rate;
        momentum_x[j] = momentum_x[j] * (1.0 - smooth_rate) + momentum_x_track[j] * smooth_rate;
        momentum_y[j] = momentum_y[j] * (1.0 - smooth_rate) + momentum_y_track[j] * smooth_rate;
    }

    ErosionResult {
        heightmap,
        discharge,
        momentum_x,
        momentum_y,
        track: discharge_track,
    }
}

/// Get terrain height with bilinear interpolation
fn get_height_interp(heightmap: &[f32], x: f32, y: f32, width: usize, height: usize) -> f32 {
    let x0 = (x.floor() as usize).min(width - 1);
    let y0 = (y.floor() as usize).min(height - 1);
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);

    let fx = x - x.floor();
    let fy = y - y.floor();

    let h00 = heightmap[y0 * width + x0];
    let h10 = heightmap[y0 * width + x1];
    let h01 = heightmap[y1 * width + x0];
    let h11 = heightmap[y1 * width + x1];

    let h0 = h00 * (1.0 - fx) + h10 * fx;
    let h1 = h01 * (1.0 - fx) + h11 * fx;

    h0 * (1.0 - fy) + h1 * fy
}

/// Calculate terrain normal (gradient) at a position
fn get_normal(heightmap: &[f32], x: f32, y: f32, width: usize, height: usize) -> (f32, f32) {
    let eps = 1.0;

    let hx0 = get_height_interp(heightmap, (x - eps).max(0.0), y, width, height);
    let hx1 = get_height_interp(heightmap, (x + eps).min((width - 1) as f32), y, width, height);
    let hy0 = get_height_interp(heightmap, x, (y - eps).max(0.0), width, height);
    let hy1 = get_height_interp(heightmap, x, (y + eps).min((height - 1) as f32), width, height);

    let dx = hx1 - hx0;
    let dy = hy1 - hy0;

    // Normalize
    let len = (dx * dx + dy * dy).sqrt();
    if len > 0.0001 {
        (dx / len, dy / len)
    } else {
        (0.0, 0.0)
    }
}

/// Get array index with bounds check
fn get_index(x: usize, y: usize, width: usize, height: usize) -> Option<usize> {
    if x < width && y < height {
        Some(y * width + x)
    } else {
        None
    }
}

/// Descend a single step - move particle downhill and perform erosion
fn descend(
    drop: &mut Drop,
    heightmap: &mut [f32],
    discharge_track: &mut [f32],
    momentum_x_track: &mut [f32],
    momentum_y_track: &mut [f32],
    discharge: &[f32],
    momentum_x: &[f32],
    momentum_y: &[f32],
    width: usize,
    height: usize,
    params: &ErosionParams,
) -> bool {
    // Get current position and height
    let (x, y) = drop.pos;

    // Bounds check
    if x < 1.0 || x >= (width - 1) as f32 || y < 1.0 || y >= (height - 1) as f32 {
        return false;
    }

    let h = get_height_interp(heightmap, x, y, width, height);

    // Get terrain gradient (points downhill)
    let (nx, ny) = get_normal(heightmap, x, y, width, height);

    // Get slope magnitude
    let slope = (nx * nx + ny * ny).sqrt().max(params.min_slope);

    // Get current cell for momentum transfer
    let idx = (y as usize) * width + (x as usize);

    // Apply momentum from the discharge map (water follows established paths)
    let momentum_effect_x = momentum_x[idx] * params.momentum_transfer;
    let momentum_effect_y = momentum_y[idx] * params.momentum_transfer;

    // Update velocity with inertia, gravity, and momentum transfer
    let accel_x = -nx * params.gravity + momentum_effect_x;
    let accel_y = -ny * params.gravity + momentum_effect_y;

    drop.vel.0 = drop.vel.0 * params.inertia + accel_x * (1.0 - params.inertia);
    drop.vel.1 = drop.vel.1 * params.inertia + accel_y * (1.0 - params.inertia);

    // Apply friction
    drop.vel.0 *= 1.0 - params.friction;
    drop.vel.1 *= 1.0 - params.friction;

    // Calculate speed and normalize movement to 1 pixel step
    let speed = (drop.vel.0 * drop.vel.0 + drop.vel.1 * drop.vel.1).sqrt();
    if speed < 0.001 {
        return false;
    }

    let step_x = drop.vel.0 / speed;
    let step_y = drop.vel.1 / speed;

    // Calculate new position
    let new_x = x + step_x;
    let new_y = y + step_y;

    // Bounds check for new position
    if new_x < 0.0 || new_x >= width as f32 || new_y < 0.0 || new_y >= height as f32 {
        return false;
    }

    // Get new height
    let new_h = get_height_interp(heightmap, new_x, new_y, width, height);
    let height_diff = h - new_h;

    // Calculate sediment capacity based on speed, slope, and volume
    let capacity = speed.max(0.1) * slope * drop.volume * params.capacity_multiplier;

    // Erosion or deposition
    if drop.sediment < capacity && height_diff > 0.0 {
        // Erode terrain
        let erosion = ((capacity - drop.sediment) * params.erosion_rate).min(height_diff);

        // Modify heightmap at current position
        let ix = x as usize;
        let iy = y as usize;
        if let Some(i) = get_index(ix, iy, width, height) {
            heightmap[i] -= erosion * 0.5;
        }
        // Also erode neighbors for smoother results
        for (ox, oy) in &[(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nx = (ix as i32 + ox) as usize;
            let ny = (iy as i32 + oy) as usize;
            if let Some(i) = get_index(nx, ny, width, height) {
                heightmap[i] -= erosion * 0.125;
            }
        }

        drop.sediment += erosion;
    } else if drop.sediment > capacity {
        // Deposit sediment
        let deposit = (drop.sediment - capacity) * params.deposition_rate;

        let ix = x as usize;
        let iy = y as usize;
        if let Some(i) = get_index(ix, iy, width, height) {
            heightmap[i] += deposit;
        }

        drop.sediment -= deposit;
    }

    // Update tracking maps for discharge and momentum
    let new_idx = (new_y as usize) * width + (new_x as usize);
    if new_idx < width * height {
        discharge_track[new_idx] += drop.volume;
        momentum_x_track[new_idx] += drop.vel.0 * drop.volume;
        momentum_y_track[new_idx] += drop.vel.1 * drop.volume;
    }

    // Evaporate
    drop.volume *= 1.0 - params.evaporation_rate;

    // Update position
    drop.pos = (new_x, new_y);

    true
}

/// Extract river segments from the discharge map
#[allow(dead_code)]
pub fn extract_rivers_from_discharge(
    discharge: &[f32],
    momentum_x: &[f32],
    momentum_y: &[f32],
    width: usize,
    height: usize,
    threshold: f32,
) -> Vec<RiverPoint> {
    let mut rivers = Vec::new();

    // Find maximum discharge for normalization
    let max_discharge = discharge.iter().copied().fold(0.0f32, f32::max);
    if max_discharge < 0.001 {
        return rivers;
    }

    // Extract cells with significant discharge
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let d = discharge[idx];

            if d >= threshold {
                // Calculate flow direction from momentum
                let mx = momentum_x[idx];
                let my = momentum_y[idx];
                let mag = (mx * mx + my * my).sqrt();

                let (dir_x, dir_y) = if mag > 0.001 {
                    (mx / mag, my / mag)
                } else {
                    (0.0, 0.0)
                };

                rivers.push(RiverPoint {
                    x: x as f32,
                    y: y as f32,
                    discharge: d,
                    normalized_discharge: d / max_discharge,
                    direction: (dir_x, dir_y),
                });
            }
        }
    }

    rivers
}

/// A point on a river with flow information
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct RiverPoint {
    pub x: f32,
    pub y: f32,
    pub discharge: f32,
    pub normalized_discharge: f32,
    pub direction: (f32, f32),
}
