//! Headless image generator for river network maps
//! Generates realistic river structures with proper flow-based thickness
//! Now includes heightmap visualization showing eroded terrain

mod terrain;
mod hydrology;
mod particle;

use terrain::{Terrain, TerrainConfig};
use hydrology::HydrologyData;
use particle::ErosionParams;
use image::{Rgb, RgbImage};
use std::path::Path;

/// Background color (dark)
const BG_COLOR: [u8; 3] = [5, 10, 15];

/// River color (light blue/white)
const RIVER_COLOR: [u8; 3] = [180, 210, 240];

/// Render the heightmap as an image with terrain coloring
fn render_heightmap(
    heightmap: &[f32],
    terrain_width: usize,
    terrain_height: usize,
    image_width: u32,
    image_height: u32,
) -> RgbImage {
    let mut img = RgbImage::new(image_width, image_height);

    let scale_x = terrain_width as f32 / image_width as f32;
    let scale_y = terrain_height as f32 / image_height as f32;

    // Find min/max for normalization
    let min_h = heightmap.iter().copied().fold(f32::INFINITY, f32::min);
    let max_h = heightmap.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let range = (max_h - min_h).max(0.001);

    for py in 0..image_height {
        for px in 0..image_width {
            // Sample heightmap with bilinear interpolation
            let tx = px as f32 * scale_x;
            let ty = py as f32 * scale_y;

            let x0 = (tx.floor() as usize).min(terrain_width - 1);
            let y0 = (ty.floor() as usize).min(terrain_height - 1);
            let x1 = (x0 + 1).min(terrain_width - 1);
            let y1 = (y0 + 1).min(terrain_height - 1);

            let fx = tx - tx.floor();
            let fy = ty - ty.floor();

            let h00 = heightmap[y0 * terrain_width + x0];
            let h10 = heightmap[y0 * terrain_width + x1];
            let h01 = heightmap[y1 * terrain_width + x0];
            let h11 = heightmap[y1 * terrain_width + x1];

            let h0 = h00 * (1.0 - fx) + h10 * fx;
            let h1 = h01 * (1.0 - fx) + h11 * fx;
            let h = h0 * (1.0 - fy) + h1 * fy;

            // Normalize height
            let normalized = ((h - min_h) / range).clamp(0.0, 1.0);

            // Calculate gradient for hillshade
            let eps = 1.0;
            let h_left = if x0 > 0 { heightmap[y0 * terrain_width + x0 - 1] } else { h };
            let h_right = if x1 < terrain_width - 1 { heightmap[y0 * terrain_width + x1 + 1] } else { h };
            let h_up = if y0 > 0 { heightmap[(y0 - 1) * terrain_width + x0] } else { h };
            let h_down = if y1 < terrain_height - 1 { heightmap[(y1 + 1) * terrain_width + x0] } else { h };

            let dx = (h_right - h_left) / (2.0 * eps);
            let dy = (h_down - h_up) / (2.0 * eps);

            // Simple hillshade lighting
            let light_x = -0.5f32;
            let light_y = -0.5f32;
            let light_z = 1.0f32;
            let light_len = (light_x * light_x + light_y * light_y + light_z * light_z).sqrt();
            let light_nx = light_x / light_len;
            let light_ny = light_y / light_len;
            let light_nz = light_z / light_len;

            let normal_len = (dx * dx + dy * dy + 1.0).sqrt();
            let nx = -dx / normal_len;
            let ny = -dy / normal_len;
            let nz = 1.0 / normal_len;

            let diffuse = (nx * light_nx + ny * light_ny + nz * light_nz).max(0.0);
            let ambient = 0.3;
            let lighting = ambient + diffuse * 0.7;

            // Color palette: dark valleys to light highlands
            let color = terrain_color(normalized);

            // Apply lighting
            let r = (color[0] as f32 * lighting).min(255.0) as u8;
            let g = (color[1] as f32 * lighting).min(255.0) as u8;
            let b = (color[2] as f32 * lighting).min(255.0) as u8;

            img.put_pixel(px, py, Rgb([r, g, b]));
        }
    }

    img
}

/// Get terrain color based on normalized height (0-1)
fn terrain_color(h: f32) -> [u8; 3] {
    // Dark blue depths to green highlands
    let deep = [5, 15, 30];
    let shallow = [10, 25, 40];
    let lowland = [15, 35, 30];
    let midland = [25, 50, 40];
    let highland = [40, 65, 55];
    let peak = [60, 80, 70];

    if h < 0.15 {
        lerp_color(&deep, &shallow, h / 0.15)
    } else if h < 0.3 {
        lerp_color(&shallow, &lowland, (h - 0.15) / 0.15)
    } else if h < 0.5 {
        lerp_color(&lowland, &midland, (h - 0.3) / 0.2)
    } else if h < 0.7 {
        lerp_color(&midland, &highland, (h - 0.5) / 0.2)
    } else {
        lerp_color(&highland, &peak, (h - 0.7) / 0.3)
    }
}

fn lerp_color(a: &[u8; 3], b: &[u8; 3], t: f32) -> [u8; 3] {
    [
        (a[0] as f32 * (1.0 - t) + b[0] as f32 * t) as u8,
        (a[1] as f32 * (1.0 - t) + b[1] as f32 * t) as u8,
        (a[2] as f32 * (1.0 - t) + b[2] as f32 * t) as u8,
    ]
}

/// Blend rivers onto heightmap image
fn blend_rivers_on_terrain(
    terrain_img: &mut RgbImage,
    hydrology: &HydrologyData,
    terrain_width: usize,
    terrain_height: usize,
) {
    let img_width = terrain_img.width() as f32;
    let img_height = terrain_img.height() as f32;
    let scale_x = img_width / terrain_width as f32;
    let scale_y = img_height / terrain_height as f32;

    // Sort segments by flow (draw smaller rivers first)
    let mut segments: Vec<_> = hydrology.river_segments.iter().collect();
    segments.sort_by(|a, b| a.flow.partial_cmp(&b.flow).unwrap());

    let max_flow = segments.iter().map(|s| s.flow).fold(0.0f32, |a, b| a.max(b));
    let log_max = max_flow.ln().max(1.0);

    for segment in &segments {
        let x0 = segment.start.0 * scale_x;
        let y0 = segment.start.1 * scale_y;
        let x1 = segment.end.0 * scale_x;
        let y1 = segment.end.1 * scale_y;

        // Normalize flow on log scale
        let flow_norm = if log_max > 0.0 {
            (segment.flow.ln().max(0.0) / log_max).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Thickness and brightness
        let thickness = 0.3 + flow_norm.powf(1.5) * 4.0;
        let brightness = 0.5 + flow_norm.powf(0.5) * 0.5;
        let alpha = 0.7 + flow_norm * 0.3;

        let color = [
            (RIVER_COLOR[0] as f32 * brightness) as u8,
            (RIVER_COLOR[1] as f32 * brightness) as u8,
            (RIVER_COLOR[2] as f32 * brightness) as u8,
        ];

        draw_line_alpha(terrain_img, x0, y0, x1, y1, thickness, color, alpha);
    }
}

/// Draw anti-aliased line with alpha blending
fn draw_line_alpha(img: &mut RgbImage, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: [u8; 3], alpha: f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.1 { return; }

    let steps = (len * 2.0).max(1.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;
        draw_circle_alpha(img, cx, cy, thickness, color, alpha);
    }
}

/// Draw filled circle with alpha blending
fn draw_circle_alpha(img: &mut RgbImage, cx: f32, cy: f32, radius: f32, color: [u8; 3], alpha: f32) {
    let r_ceil = radius.ceil() as i32 + 1;
    let width = img.width() as i32;
    let height = img.height() as i32;

    for dy in -r_ceil..=r_ceil {
        for dx in -r_ceil..=r_ceil {
            let px = cx as i32 + dx;
            let py = cy as i32 + dy;
            if px >= 0 && px < width && py >= 0 && py < height {
                let dist = ((dx as f32).powi(2) + (dy as f32).powi(2)).sqrt();
                if dist <= radius + 0.5 {
                    let edge_alpha = if dist <= radius - 0.5 { alpha } else { alpha * (1.0 - (dist - radius + 0.5)) };
                    if edge_alpha > 0.0 {
                        let pixel = img.get_pixel_mut(px as u32, py as u32);
                        pixel[0] = ((pixel[0] as f32 * (1.0 - edge_alpha) + color[0] as f32 * edge_alpha) as u8).min(255);
                        pixel[1] = ((pixel[1] as f32 * (1.0 - edge_alpha) + color[1] as f32 * edge_alpha) as u8).min(255);
                        pixel[2] = ((pixel[2] as f32 * (1.0 - edge_alpha) + color[2] as f32 * edge_alpha) as u8).min(255);
                    }
                }
            }
        }
    }
}

/// Render river network to an image - simple monochrome with thickness based on flow
fn render_river_image(
    hydrology: &HydrologyData,
    width: u32,
    height: u32,
    terrain_width: usize,
    terrain_height: usize,
) -> RgbImage {
    let mut img = RgbImage::from_pixel(width, height, Rgb(BG_COLOR));

    // Sort segments by flow (draw smaller rivers first, larger on top)
    let mut segments: Vec<_> = hydrology.river_segments.iter().collect();
    segments.sort_by(|a, b| a.flow.partial_cmp(&b.flow).unwrap());

    let max_flow = segments.iter().map(|s| s.flow).fold(0.0f32, |a, b| a.max(b));
    let log_max = max_flow.ln();

    let img_width = img.width() as f32;
    let img_height = img.height() as f32;
    let scale_x = img_width / terrain_width as f32;
    let scale_y = img_height / terrain_height as f32;

    // Draw each river segment
    for segment in &segments {
        let x0 = segment.start.0 * scale_x;
        let y0 = segment.start.1 * scale_y;
        let x1 = segment.end.0 * scale_x;
        let y1 = segment.end.1 * scale_y;

        // Normalize flow on log scale
        let flow_norm = if log_max > 0.0 {
            (segment.flow.ln().max(0.0) / log_max).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Thickness: very thin (0.2px) for capillaries, thick (8px) for main rivers
        // Power of 2 gives good balance between tributaries and main channels
        let thickness = 0.2 + flow_norm.powf(2.0) * 8.0;

        // Brightness: slightly dim for tiny streams, bright for main rivers
        let brightness = 0.3 + flow_norm.powf(0.5) * 0.7;

        let color = [
            (RIVER_COLOR[0] as f32 * brightness) as u8,
            (RIVER_COLOR[1] as f32 * brightness) as u8,
            (RIVER_COLOR[2] as f32 * brightness) as u8,
        ];

        draw_line(&mut img, x0, y0, x1, y1, thickness, color);
    }

    img
}

/// Draw anti-aliased line with given thickness
fn draw_line(img: &mut RgbImage, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: [u8; 3]) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.1 { return; }

    // For very thin lines, use Bresenham-style drawing
    if thickness < 0.5 {
        draw_thin_line(img, x0, y0, x1, y1, color, thickness * 2.0);
        return;
    }

    let steps = (len * 2.0).max(1.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;
        draw_circle(img, cx, cy, thickness, color);
    }
}

/// Draw a thin line with alpha based on thickness
fn draw_thin_line(img: &mut RgbImage, x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 3], alpha: f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = (dx.abs().max(dy.abs()) * 2.0).max(1.0) as i32;

    let width = img.width() as i32;
    let height = img.height() as i32;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 + dx * t;
        let y = y0 + dy * t;

        let px = x as i32;
        let py = y as i32;

        if px >= 0 && px < width && py >= 0 && py < height {
            let pixel = img.get_pixel_mut(px as u32, py as u32);
            pixel[0] = ((pixel[0] as f32 * (1.0 - alpha) + color[0] as f32 * alpha) as u8).min(255);
            pixel[1] = ((pixel[1] as f32 * (1.0 - alpha) + color[1] as f32 * alpha) as u8).min(255);
            pixel[2] = ((pixel[2] as f32 * (1.0 - alpha) + color[2] as f32 * alpha) as u8).min(255);
        }
    }
}

/// Draw filled circle with anti-aliasing
fn draw_circle(img: &mut RgbImage, cx: f32, cy: f32, radius: f32, color: [u8; 3]) {
    let r_ceil = radius.ceil() as i32 + 1;
    let width = img.width() as i32;
    let height = img.height() as i32;

    for dy in -r_ceil..=r_ceil {
        for dx in -r_ceil..=r_ceil {
            let px = cx as i32 + dx;
            let py = cy as i32 + dy;
            if px >= 0 && px < width && py >= 0 && py < height {
                let dist = ((dx as f32).powi(2) + (dy as f32).powi(2)).sqrt();
                if dist <= radius + 0.5 {
                    let alpha = if dist <= radius - 0.5 { 1.0 } else { 1.0 - (dist - radius + 0.5) };
                    if alpha > 0.0 {
                        let pixel = img.get_pixel_mut(px as u32, py as u32);
                        pixel[0] = ((pixel[0] as f32 * (1.0 - alpha) + color[0] as f32 * alpha) as u8).min(255);
                        pixel[1] = ((pixel[1] as f32 * (1.0 - alpha) + color[1] as f32 * alpha) as u8).min(255);
                        pixel[2] = ((pixel[2] as f32 * (1.0 - alpha) + color[2] as f32 * alpha) as u8).min(255);
                    }
                }
            }
        }
    }
}

fn generate_image(seed: u32, output_path: &Path, use_erosion: bool, show_terrain: bool) {
    println!("Generating river network with seed {} ({}{})...",
        seed,
        if use_erosion { "particle erosion" } else { "D8 flow" },
        if show_terrain { " + terrain" } else { "" });

    let terrain_size = 512;
    let image_size = 2048;
    let flow_threshold = 50.0;

    let config = TerrainConfig {
        width: terrain_size,
        height: terrain_size,
        seed,
        ..Default::default()
    };
    let terrain = Terrain::generate(config);

    let hydrology = if use_erosion {
        // Use particle-based erosion for realistic rivers with meandering
        let erosion_params = ErosionParams {
            iterations: 80_000,
            ..Default::default()
        };
        HydrologyData::simulate_with_erosion(&terrain, flow_threshold, &erosion_params, seed)
    } else {
        // Use D8 flow direction
        HydrologyData::simulate(&terrain, flow_threshold)
    };

    println!("  {} river segments", hydrology.river_segments.len());

    let img = if show_terrain {
        // Use eroded heightmap if available, otherwise original
        let heightmap = hydrology.eroded_heightmap.as_ref()
            .unwrap_or(&terrain.heightmap);

        // Render terrain with rivers overlaid
        let mut terrain_img = render_heightmap(
            heightmap,
            terrain_size,
            terrain_size,
            image_size,
            image_size,
        );

        // Blend rivers on top
        blend_rivers_on_terrain(&mut terrain_img, &hydrology, terrain_size, terrain_size);
        terrain_img
    } else {
        // Just rivers on dark background
        render_river_image(&hydrology, image_size, image_size, terrain_size, terrain_size)
    };

    img.save(output_path).expect("Failed to save image");
    println!("  Saved to {:?}", output_path);
}

fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     Procedural River Network - Image Generator                  ║");
    println!("║     Using particle-based hydraulic erosion with meandering      ║");
    println!("║     Based on Nick McDonald's Procedural Hydrology               ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();

    let images_dir = Path::new("images");
    std::fs::create_dir_all(images_dir).expect("Failed to create images directory");

    // Generate with particle erosion showing both terrain and rivers
    let seeds = [42, 1337, 2024];
    for (i, &seed) in seeds.iter().enumerate() {
        // Generate terrain + rivers image
        generate_image(seed, &images_dir.join(format!("terrain_rivers_{}.png", i + 1)), true, true);
    }

    // Hero image with terrain
    generate_image(42, &images_dir.join("hero_terrain.png"), true, true);

    // Also generate rivers-only version
    generate_image(42, &images_dir.join("hero.png"), true, false);

    println!("\nDone! Generated images with particle erosion and heightmap visualization.");
}
