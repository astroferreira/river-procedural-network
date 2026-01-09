//! Terrain generation using multi-octave noise
//! Creates realistic heightmaps with natural drainage patterns

use noise::{NoiseFn, Perlin, Fbm, MultiFractal};
use rand::Rng;

/// Terrain generator configuration
#[derive(Clone)]
pub struct TerrainConfig {
    pub width: usize,
    pub height: usize,
    pub seed: u32,
    pub scale: f64,
    pub octaves: usize,
    pub persistence: f64,
    pub lacunarity: f64,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 1024,
            seed: 42,
            scale: 0.004,
            octaves: 8,
            persistence: 0.5,
            lacunarity: 2.0,
        }
    }
}

/// Generated terrain data
pub struct Terrain {
    pub width: usize,
    pub height: usize,
    pub heightmap: Vec<f32>,
    pub config: TerrainConfig,
}

impl Terrain {
    /// Generate a new terrain from configuration
    pub fn generate(config: TerrainConfig) -> Self {
        let width = config.width;
        let height = config.height;
        let scale = config.scale;

        // Use seed to create varied but controlled randomness
        let seed_f = config.seed as f64;

        // Very large scale noise for major drainage divides (mountain ranges)
        let mountain: Fbm<Perlin> = Fbm::new(config.seed)
            .set_octaves(2)
            .set_persistence(0.5)
            .set_lacunarity(2.0);

        // Medium scale for valleys and sub-basins
        let valley: Fbm<Perlin> = Fbm::new(config.seed.wrapping_add(100))
            .set_octaves(4)
            .set_persistence(0.45)
            .set_lacunarity(2.0);

        // Fine detail noise - creates small tributaries
        let detail: Fbm<Perlin> = Fbm::new(config.seed.wrapping_add(200))
            .set_octaves(6)
            .set_persistence(0.5)
            .set_lacunarity(2.5);

        // Very fine noise for micro-tributaries
        let micro: Fbm<Perlin> = Fbm::new(config.seed.wrapping_add(300))
            .set_octaves(4)
            .set_persistence(0.6)
            .set_lacunarity(3.0);

        // Determine overall tilt direction from seed - this creates the main drainage
        let tilt_angle = (seed_f * 0.618).sin() * std::f64::consts::PI * 2.0;
        let tilt_x = tilt_angle.cos();
        let tilt_y = tilt_angle.sin();

        // Secondary tilt for more complex drainage
        let tilt2_angle = tilt_angle + std::f64::consts::PI * 0.4;
        let tilt2_x = tilt2_angle.cos();
        let tilt2_y = tilt2_angle.sin();

        let mut heightmap = Vec::with_capacity(width * height);

        for y in 0..height {
            for x in 0..width {
                // Normalized coordinates (0 to 1)
                let nx = x as f64 / width as f64;
                let ny = y as f64 / height as f64;

                // Noise coordinates at different scales
                let sx = x as f64 * scale;
                let sy = y as f64 * scale;

                // PRIMARY: Strong continental tilt - creates main drainage direction
                // This is the dominant factor ensuring rivers flow in consistent direction
                let tilt = (nx - 0.5) * tilt_x + (ny - 0.5) * tilt_y;
                let base_height = 0.5 + tilt * 0.5;  // Strong gradient

                // SECONDARY: Mountain ridges perpendicular to main drainage
                // Creates major drainage divides that funnel water into main channels
                let mountain_val = mountain.get([sx * 0.15, sy * 0.15]);
                // Make mountains as ridges perpendicular to tilt
                let ridge_factor = ((nx - 0.5) * tilt2_x + (ny - 0.5) * tilt2_y).abs();
                let mountain_height = mountain_val.abs() * 0.25 * (0.3 + ridge_factor);

                // TERTIARY: Valley carving - creates sub-basins
                let valley_val = valley.get([sx * 0.5, sy * 0.5]);
                let valley_height = valley_val * 0.12;

                // DETAIL: Small-scale roughness for tributaries
                let detail_val = detail.get([sx * 1.5, sy * 1.5]) * 0.06;

                // MICRO: Very fine detail for tiny tributaries
                let micro_val = micro.get([sx * 4.0, sy * 4.0]) * 0.025;

                // Edge falloff - rivers drain to edges
                let edge_dist = (nx.min(1.0 - nx).min(ny.min(1.0 - ny)) * 2.0).min(1.0);
                let edge_falloff = edge_dist.powf(0.5);

                // Combine: base gradient dominates, smaller features add drainage detail
                let h = base_height + mountain_height + valley_height + detail_val + micro_val;
                let h = h * edge_falloff;

                // Normalize to 0-1 range
                heightmap.push(h.clamp(0.0, 1.0) as f32);
            }
        }

        Self {
            width,
            height,
            heightmap,
            config,
        }
    }

    /// Get height at a specific position
    #[inline]
    pub fn get_height(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.heightmap[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get height with bilinear interpolation
    pub fn get_height_interpolated(&self, x: f32, y: f32) -> f32 {
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let fx = x - x.floor();
        let fy = y - y.floor();

        let h00 = self.get_height(x0, y0);
        let h10 = self.get_height(x1, y0);
        let h01 = self.get_height(x0, y1);
        let h11 = self.get_height(x1, y1);

        let h0 = h00 * (1.0 - fx) + h10 * fx;
        let h1 = h01 * (1.0 - fx) + h11 * fx;

        h0 * (1.0 - fy) + h1 * fy
    }

    /// Calculate slope at a position
    pub fn get_slope(&self, x: usize, y: usize) -> f32 {
        if x == 0 || x >= self.width - 1 || y == 0 || y >= self.height - 1 {
            return 0.0;
        }

        let dx = self.get_height(x + 1, y) - self.get_height(x - 1, y);
        let dy = self.get_height(x, y + 1) - self.get_height(x, y - 1);

        (dx * dx + dy * dy).sqrt()
    }
}

/// Generate a random terrain with a random seed
pub fn generate_random_terrain(width: usize, height: usize) -> Terrain {
    let mut rng = rand::thread_rng();
    let config = TerrainConfig {
        width,
        height,
        seed: rng.gen(),
        ..Default::default()
    };
    Terrain::generate(config)
}
