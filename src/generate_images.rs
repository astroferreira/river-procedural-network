//! Headless image generator for river network maps
//! Generates beautiful PNG images without requiring a GPU or display

mod terrain;
mod hydrology;

use terrain::{Terrain, TerrainConfig};
use hydrology::{HydrologyData, RiverSegment};
use image::{Rgb, RgbImage};
use std::path::Path;

/// Color palette for watersheds (RGB values)
const WATERSHED_COLORS: &[[u8; 3]] = &[
    [242, 51, 102],   // Bright red/pink
    [51, 204, 153],   // Teal/cyan
    [230, 179, 26],   // Gold/yellow
    [77, 128, 242],   // Blue
    [204, 77, 204],   // Magenta/purple
    [26, 230, 102],   // Bright green
    [242, 128, 51],   // Orange
    [102, 230, 230],  // Cyan
    [230, 102, 153],  // Pink
    [153, 204, 51],   // Lime
    [128, 77, 230],   // Purple
    [51, 179, 204],   // Sky blue
    [230, 153, 128],  // Salmon
    [77, 230, 179],   // Mint
    [204, 204, 77],   // Yellow-green
    [179, 102, 179],  // Lavender
];

/// Background color (dark blue-green)
const BG_COLOR: [u8; 3] = [5, 13, 20];

/// Render river network to an image
fn render_river_image(
    hydrology: &HydrologyData,
    width: u32,
    height: u32,
    terrain_width: usize,
    terrain_height: usize,
) -> RgbImage {
    let mut img = RgbImage::from_pixel(width, height, Rgb(BG_COLOR));

    // Add subtle background variation
    add_background_texture(&mut img, terrain_width as u32);

    // Sort segments by flow (draw smaller rivers first, larger on top)
    let mut segments: Vec<&RiverSegment> = hydrology.river_segments.iter().collect();
    segments.sort_by(|a, b| a.flow.partial_cmp(&b.flow).unwrap());

    let max_flow = segments.iter().map(|s| s.flow).fold(0.0f32, |a, b| a.max(b));

    // Draw each river segment
    for segment in segments {
        draw_river_segment(&mut img, segment, terrain_width, terrain_height, max_flow);
    }

    // Add glow effect for major rivers
    add_glow_effect(&mut img, &hydrology.river_segments, terrain_width, terrain_height, max_flow);

    img
}

/// Add subtle background texture
fn add_background_texture(img: &mut RgbImage, seed: u32) {
    let width = img.width();
    let height = img.height();

    for y in 0..height {
        for x in 0..width {
            // Simple noise-like variation
            let noise = simple_noise(x as f32 / 100.0, y as f32 / 100.0, seed);
            let variation = (noise * 8.0) as i32;

            let pixel = img.get_pixel_mut(x, y);
            pixel[0] = (pixel[0] as i32 + variation).clamp(0, 30) as u8;
            pixel[1] = (pixel[1] as i32 + variation).clamp(0, 30) as u8;
            pixel[2] = (pixel[2] as i32 + variation / 2).clamp(0, 40) as u8;
        }
    }
}

/// Simple noise function for background
fn simple_noise(x: f32, y: f32, seed: u32) -> f32 {
    let n = (x * 12.9898 + y * 78.233 + seed as f32 * 0.1).sin() * 43758.5453;
    n.fract()
}

/// Draw a single river segment with anti-aliasing
fn draw_river_segment(
    img: &mut RgbImage,
    segment: &RiverSegment,
    terrain_width: usize,
    terrain_height: usize,
    max_flow: f32,
) {
    let img_width = img.width() as f32;
    let img_height = img.height() as f32;

    // Convert terrain coordinates to image coordinates
    let x0 = (segment.start.0 / terrain_width as f32) * img_width;
    let y0 = (segment.start.1 / terrain_height as f32) * img_height;
    let x1 = (segment.end.0 / terrain_width as f32) * img_width;
    let y1 = (segment.end.1 / terrain_height as f32) * img_height;

    // Get watershed color
    let color_idx = (segment.watershed_id as usize) % WATERSHED_COLORS.len();
    let base_color = WATERSHED_COLORS[color_idx];

    // Calculate line thickness based on flow
    let flow_normalized = (segment.flow / max_flow).min(1.0);
    let thickness = 0.5 + flow_normalized * 3.0;

    // Calculate brightness based on flow
    let brightness = 0.4 + flow_normalized * 0.6;

    // Apply brightness to color
    let color = [
        (base_color[0] as f32 * brightness) as u8,
        (base_color[1] as f32 * brightness) as u8,
        (base_color[2] as f32 * brightness) as u8,
    ];

    // Draw thick anti-aliased line
    draw_thick_line(img, x0, y0, x1, y1, thickness, color);
}

/// Draw a thick anti-aliased line
fn draw_thick_line(
    img: &mut RgbImage,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    color: [u8; 3],
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();

    if len < 0.1 {
        return;
    }

    // Number of steps for drawing
    let steps = (len * 2.0).ceil() as i32;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;

        // Draw filled circle at each point
        draw_filled_circle(img, cx, cy, thickness, color);
    }
}

/// Draw a filled circle with anti-aliasing
fn draw_filled_circle(img: &mut RgbImage, cx: f32, cy: f32, radius: f32, color: [u8; 3]) {
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
                    let alpha = if dist <= radius - 0.5 {
                        1.0
                    } else {
                        1.0 - (dist - radius + 0.5)
                    };

                    if alpha > 0.0 {
                        let pixel = img.get_pixel_mut(px as u32, py as u32);
                        pixel[0] = blend_channel(pixel[0], color[0], alpha);
                        pixel[1] = blend_channel(pixel[1], color[1], alpha);
                        pixel[2] = blend_channel(pixel[2], color[2], alpha);
                    }
                }
            }
        }
    }
}

/// Blend a single color channel
fn blend_channel(dst: u8, src: u8, alpha: f32) -> u8 {
    ((dst as f32 * (1.0 - alpha) + src as f32 * alpha) as u8).min(255)
}

/// Add glow effect around major rivers
fn add_glow_effect(
    img: &mut RgbImage,
    segments: &[RiverSegment],
    terrain_width: usize,
    terrain_height: usize,
    max_flow: f32,
) {
    let img_width = img.width() as f32;
    let img_height = img.height() as f32;

    // Only add glow for high-flow segments
    let threshold = max_flow * 0.3;

    for segment in segments.iter().filter(|s| s.flow > threshold) {
        let x0 = (segment.start.0 / terrain_width as f32) * img_width;
        let y0 = (segment.start.1 / terrain_height as f32) * img_height;
        let x1 = (segment.end.0 / terrain_width as f32) * img_width;
        let y1 = (segment.end.1 / terrain_height as f32) * img_height;

        let color_idx = (segment.watershed_id as usize) % WATERSHED_COLORS.len();
        let base_color = WATERSHED_COLORS[color_idx];

        // Dimmer glow color
        let glow_color = [
            (base_color[0] as f32 * 0.3) as u8,
            (base_color[1] as f32 * 0.3) as u8,
            (base_color[2] as f32 * 0.3) as u8,
        ];

        let flow_normalized = (segment.flow / max_flow).min(1.0);
        let glow_radius = 2.0 + flow_normalized * 4.0;

        draw_thick_line(img, x0, y0, x1, y1, glow_radius, glow_color);
    }
}

/// Generate and save a river map image
fn generate_image(seed: u32, output_path: &Path) {
    println!("Generating river network with seed {}...", seed);

    let terrain_size = 1024;
    let image_size = 1920;
    let flow_threshold = 80.0;

    // Generate terrain
    let config = TerrainConfig {
        width: terrain_size,
        height: terrain_size,
        seed,
        ..Default::default()
    };
    let terrain = Terrain::generate(config);

    // Run hydrology simulation
    let hydrology = HydrologyData::simulate(&terrain, flow_threshold);

    println!(
        "  {} watersheds, {} river segments",
        hydrology.num_watersheds,
        hydrology.river_segments.len()
    );

    // Render image
    let img = render_river_image(
        &hydrology,
        image_size,
        image_size,
        terrain_size,
        terrain_size,
    );

    // Save image
    img.save(output_path).expect("Failed to save image");
    println!("  Saved to {:?}", output_path);
}

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║     Procedural River Network - Image Generator         ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();

    // Create images directory
    let images_dir = Path::new("images");
    std::fs::create_dir_all(images_dir).expect("Failed to create images directory");

    // Generate several sample images with different seeds
    let seeds = [42, 1337, 2024, 8675309, 12345];

    for (i, &seed) in seeds.iter().enumerate() {
        let filename = format!("river_network_{}.png", i + 1);
        let output_path = images_dir.join(&filename);
        generate_image(seed, &output_path);
    }

    // Generate a hero image for the README
    generate_image(42, &images_dir.join("hero.png"));

    println!();
    println!("Done! Generated {} images in the 'images' directory.", seeds.len() + 1);
}
