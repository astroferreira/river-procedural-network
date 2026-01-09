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
    pub mountain_influence: f64,
    pub valley_carving: f64,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 1024,
            seed: 42,
            scale: 0.003,
            octaves: 8,
            persistence: 0.5,
            lacunarity: 2.0,
            mountain_influence: 0.6,
            valley_carving: 0.4,
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
        log::info!("Generating terrain {}x{} with seed {}",
            config.width, config.height, config.seed);

        let fbm: Fbm<Perlin> = Fbm::new(config.seed)
            .set_octaves(config.octaves)
            .set_persistence(config.persistence)
            .set_lacunarity(config.lacunarity);

        // Secondary noise for mountain ridges
        let ridge_noise: Fbm<Perlin> = Fbm::new(config.seed.wrapping_add(1000))
            .set_octaves(4)
            .set_persistence(0.6)
            .set_lacunarity(2.2);

        // Tertiary noise for valley carving
        let valley_noise: Fbm<Perlin> = Fbm::new(config.seed.wrapping_add(2000))
            .set_octaves(6)
            .set_persistence(0.45)
            .set_lacunarity(2.1);

        let width = config.width;
        let height = config.height;
        let scale = config.scale;
        let mountain_influence = config.mountain_influence;
        let valley_carving = config.valley_carving;

        // Generate heightmap
        let mut heightmap = Vec::with_capacity(width * height);

        for y in 0..height {
            for x in 0..width {
                let nx = x as f64 * scale;
                let ny = y as f64 * scale;

                // Base terrain
                let base = fbm.get([nx, ny]);

                // Ridge mountains (absolute value creates ridges)
                let ridge = (ridge_noise.get([nx * 0.5, ny * 0.5])).abs();
                let ridge_contribution = ridge * mountain_influence;

                // Valley carving (squared creates valleys)
                let valley = valley_noise.get([nx * 0.7, ny * 0.7]);
                let valley_contribution = valley * valley * valley_carving;

                // Combine with edge falloff for island-like appearance
                let cx = (x as f64 / width as f64) * 2.0 - 1.0;
                let cy = (y as f64 / height as f64) * 2.0 - 1.0;
                let edge_dist = (cx * cx + cy * cy).sqrt();
                let edge_falloff = 1.0 - (edge_dist * 0.7).min(1.0).powf(2.0);

                // Final height combining all factors
                let combined = (base + ridge_contribution - valley_contribution) * edge_falloff;

                // Normalize to 0-1 range
                heightmap.push(((combined + 1.0) / 2.0).clamp(0.0, 1.0) as f32);
            }
        }

        log::info!("Terrain generation complete");

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
