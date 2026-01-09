//! Headless image generator for river network maps
//! Generates realistic river structures with proper flow-based thickness

mod terrain;
mod hydrology;

use terrain::{Terrain, TerrainConfig};
use hydrology::HydrologyData;
use image::{Rgb, RgbImage};
use std::path::Path;

/// Background color (dark)
const BG_COLOR: [u8; 3] = [5, 10, 15];

/// River color (light blue/white)
const RIVER_COLOR: [u8; 3] = [180, 210, 240];

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

fn generate_image(seed: u32, output_path: &Path) {
    println!("Generating river network with seed {}...", seed);

    let terrain_size = 1024;
    let image_size = 2048;
    let flow_threshold = 6.0;  // Captures fine tributaries + boosted main rivers
    let num_sources = 200;     // Number of main river source points

    let config = TerrainConfig {
        width: terrain_size,
        height: terrain_size,
        seed,
        ..Default::default()
    };
    let terrain = Terrain::generate(config);

    // Use hybrid model: capillary tributaries + boosted main rivers from sources
    let hydrology = HydrologyData::simulate_with_sources(
        &terrain,
        flow_threshold,
        seed,
        num_sources
    );

    println!("  {} river segments", hydrology.river_segments.len());

    let img = render_river_image(&hydrology, image_size, image_size, terrain_size, terrain_size);
    img.save(output_path).expect("Failed to save image");
    println!("  Saved to {:?}", output_path);
}

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║     Procedural River Network - Image Generator         ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();

    let images_dir = Path::new("images");
    std::fs::create_dir_all(images_dir).expect("Failed to create images directory");

    let seeds = [42, 1337, 2024, 8675309, 12345];
    for (i, &seed) in seeds.iter().enumerate() {
        generate_image(seed, &images_dir.join(format!("river_network_{}.png", i + 1)));
    }
    generate_image(42, &images_dir.join("hero.png"));

    println!("\nDone! Generated {} images.", seeds.len() + 1);
}
