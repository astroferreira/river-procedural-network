//! Headless image generator for river network maps
//! Generates beautiful PNG images without requiring a GPU or display

mod terrain;
mod hydrology;

use terrain::{Terrain, TerrainConfig};
use hydrology::{HydrologyData, RiverSegment};
use image::{Rgb, RgbImage};
use std::path::Path;

/// Color palette for watersheds (RGB values) - vibrant colors like the reference
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
const BG_COLOR: [u8; 3] = [8, 18, 25];

/// Major outlet point for spatial coloring
struct Outlet {
    x: f32,
    y: f32,
    flow: f32,
    color_idx: usize,
}

/// Find major river outlets and create spatial regions
fn find_major_outlets(
    hydrology: &HydrologyData,
    terrain_width: usize,
    terrain_height: usize,
) -> Vec<Outlet> {
    let mut outlets: Vec<Outlet> = Vec::new();

    // Collect all edge segments with high flow
    for segment in &hydrology.river_segments {
        let near_edge = segment.end.0 < 3.0
            || segment.end.0 > (terrain_width - 3) as f32
            || segment.end.1 < 3.0
            || segment.end.1 > (terrain_height - 3) as f32;

        if near_edge && segment.flow > 3.5 {
            // Check if this outlet is far enough from existing ones
            let min_dist = outlets.iter()
                .map(|o| ((o.x - segment.end.0).powi(2) + (o.y - segment.end.1).powi(2)).sqrt())
                .fold(f32::MAX, |a, b| a.min(b));

            if min_dist > 50.0 {  // Minimum distance between outlets
                outlets.push(Outlet {
                    x: segment.end.0,
                    y: segment.end.1,
                    flow: segment.flow,
                    color_idx: 0,
                });
            } else {
                // Merge with nearest if this has higher flow
                for outlet in &mut outlets {
                    let dist = ((outlet.x - segment.end.0).powi(2) + (outlet.y - segment.end.1).powi(2)).sqrt();
                    if dist < 50.0 && segment.flow > outlet.flow {
                        outlet.x = segment.end.0;
                        outlet.y = segment.end.1;
                        outlet.flow = segment.flow;
                    }
                }
            }
        }
    }

    // Sort by flow and assign colors
    outlets.sort_by(|a, b| b.flow.partial_cmp(&a.flow).unwrap());
    outlets.truncate(16); // Keep top 16 outlets

    // Assign colors based on position for spatial coherence
    for (i, outlet) in outlets.iter_mut().enumerate() {
        outlet.color_idx = i % WATERSHED_COLORS.len();
    }

    // If too few outlets, add grid points
    if outlets.len() < 8 {
        let grid = 3;
        for gy in 0..grid {
            for gx in 0..grid {
                let x = (gx as f32 + 0.5) * terrain_width as f32 / grid as f32;
                let y = (gy as f32 + 0.5) * terrain_height as f32 / grid as f32;
                outlets.push(Outlet {
                    x,
                    y,
                    flow: 1.0,
                    color_idx: (gy * grid + gx) % WATERSHED_COLORS.len(),
                });
            }
        }
    }

    outlets
}

/// Get color index for a point based on nearest outlet (Voronoi-style)
fn get_region_color(x: f32, y: f32, outlets: &[Outlet]) -> usize {
    let mut best_idx = 0;
    let mut best_score = f32::MAX;

    for (i, outlet) in outlets.iter().enumerate() {
        let dx = x - outlet.x;
        let dy = y - outlet.y;
        let dist = (dx * dx + dy * dy).sqrt();
        // Weight by flow - higher flow outlets have larger "reach"
        let score = dist / (1.0 + outlet.flow.ln().max(0.0) * 0.3);

        if score < best_score {
            best_score = score;
            best_idx = i;
        }
    }

    outlets[best_idx].color_idx
}

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

    // Find major outlets for spatial coloring
    let outlets = find_major_outlets(hydrology, terrain_width, terrain_height);

    // Sort segments by flow (draw smaller rivers first, larger on top)
    let mut segments: Vec<&RiverSegment> = hydrology.river_segments.iter().collect();
    segments.sort_by(|a, b| a.flow.partial_cmp(&b.flow).unwrap());

    let max_flow = segments.iter().map(|s| s.flow).fold(0.0f32, |a, b| a.max(b));
    let log_max = max_flow.ln();

    // First pass: Draw glow for rivers
    for segment in &segments {
        let flow_normalized = if log_max > 0.0 {
            (segment.flow.ln() / log_max).max(0.0).min(1.0)
        } else {
            0.0
        };

        if flow_normalized > 0.2 {
            draw_river_glow(&mut img, segment, terrain_width, terrain_height,
                           flow_normalized, &outlets);
        }
    }

    // Second pass: Draw each river segment
    for segment in &segments {
        draw_river_segment(&mut img, segment, terrain_width, terrain_height,
                          log_max, &outlets);
    }

    img
}

/// Add subtle background texture
fn add_background_texture(img: &mut RgbImage, seed: u32) {
    let width = img.width();
    let height = img.height();

    for y in 0..height {
        for x in 0..width {
            let noise = simple_noise(x as f32 / 80.0, y as f32 / 80.0, seed);
            let variation = (noise * 10.0) as i32;

            let pixel = img.get_pixel_mut(x, y);
            pixel[0] = (pixel[0] as i32 + variation).clamp(0, 35) as u8;
            pixel[1] = (pixel[1] as i32 + variation).clamp(0, 40) as u8;
            pixel[2] = (pixel[2] as i32 + variation / 2).clamp(0, 45) as u8;
        }
    }
}

fn simple_noise(x: f32, y: f32, seed: u32) -> f32 {
    let n = (x * 12.9898 + y * 78.233 + seed as f32 * 0.1).sin() * 43758.5453;
    n.fract()
}

/// Draw glow effect for a river segment
fn draw_river_glow(
    img: &mut RgbImage,
    segment: &RiverSegment,
    terrain_width: usize,
    terrain_height: usize,
    flow_normalized: f32,
    outlets: &[Outlet],
) {
    let img_width = img.width() as f32;
    let img_height = img.height() as f32;

    let x0 = (segment.start.0 / terrain_width as f32) * img_width;
    let y0 = (segment.start.1 / terrain_height as f32) * img_height;
    let x1 = (segment.end.0 / terrain_width as f32) * img_width;
    let y1 = (segment.end.1 / terrain_height as f32) * img_height;

    // Get color based on spatial region
    let mid_x = (segment.start.0 + segment.end.0) / 2.0;
    let mid_y = (segment.start.1 + segment.end.1) / 2.0;
    let color_idx = get_region_color(mid_x, mid_y, outlets);
    let base_color = WATERSHED_COLORS[color_idx];

    // Subtle glow only for larger rivers
    let glow_alpha = 0.06 + flow_normalized * 0.12;
    let glow_radius = 2.0 + flow_normalized * 8.0;

    let glow_color = [
        (base_color[0] as f32 * glow_alpha) as u8,
        (base_color[1] as f32 * glow_alpha) as u8,
        (base_color[2] as f32 * glow_alpha) as u8,
    ];

    draw_thick_line_additive(img, x0, y0, x1, y1, glow_radius, glow_color);
}

/// Draw a single river segment
fn draw_river_segment(
    img: &mut RgbImage,
    segment: &RiverSegment,
    terrain_width: usize,
    terrain_height: usize,
    log_max: f32,
    outlets: &[Outlet],
) {
    let img_width = img.width() as f32;
    let img_height = img.height() as f32;

    let x0 = (segment.start.0 / terrain_width as f32) * img_width;
    let y0 = (segment.start.1 / terrain_height as f32) * img_height;
    let x1 = (segment.end.0 / terrain_width as f32) * img_width;
    let y1 = (segment.end.1 / terrain_height as f32) * img_height;

    // Get color based on spatial region
    let mid_x = (segment.start.0 + segment.end.0) / 2.0;
    let mid_y = (segment.start.1 + segment.end.1) / 2.0;
    let color_idx = get_region_color(mid_x, mid_y, outlets);
    let base_color = WATERSHED_COLORS[color_idx];

    let flow_normalized = if log_max > 0.0 {
        (segment.flow.ln() / log_max).max(0.0).min(1.0)
    } else {
        0.0
    };

    // Thickness based on flow - very thin for small tributaries, thick for main rivers
    // Use steeper power curve for more dramatic difference
    let thickness = 0.15 + flow_normalized.powf(1.5) * 5.0;

    // Brightness based on flow - small rivers dimmer but still visible
    let brightness = 0.5 + flow_normalized * 0.5;

    let color = [
        (base_color[0] as f32 * brightness).min(255.0) as u8,
        (base_color[1] as f32 * brightness).min(255.0) as u8,
        (base_color[2] as f32 * brightness).min(255.0) as u8,
    ];

    draw_thick_line(img, x0, y0, x1, y1, thickness, color);
}

fn draw_thick_line(img: &mut RgbImage, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: [u8; 3]) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.1 { return; }

    let steps = (len * 2.0).ceil() as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        draw_filled_circle(img, x0 + dx * t, y0 + dy * t, thickness, color);
    }
}

fn draw_thick_line_additive(img: &mut RgbImage, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: [u8; 3]) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.1 { return; }

    let steps = (len * 1.5).ceil() as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        draw_filled_circle_additive(img, x0 + dx * t, y0 + dy * t, thickness, color);
    }
}

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

fn draw_filled_circle_additive(img: &mut RgbImage, cx: f32, cy: f32, radius: f32, color: [u8; 3]) {
    let r_ceil = radius.ceil() as i32 + 1;
    let width = img.width() as i32;
    let height = img.height() as i32;

    for dy in -r_ceil..=r_ceil {
        for dx in -r_ceil..=r_ceil {
            let px = cx as i32 + dx;
            let py = cy as i32 + dy;
            if px >= 0 && px < width && py >= 0 && py < height {
                let dist = ((dx as f32).powi(2) + (dy as f32).powi(2)).sqrt();
                if dist <= radius {
                    let falloff = 1.0 - (dist / radius);
                    let alpha = falloff * falloff;
                    if alpha > 0.01 {
                        let pixel = img.get_pixel_mut(px as u32, py as u32);
                        pixel[0] = (pixel[0] as f32 + color[0] as f32 * alpha).min(255.0) as u8;
                        pixel[1] = (pixel[1] as f32 + color[1] as f32 * alpha).min(255.0) as u8;
                        pixel[2] = (pixel[2] as f32 + color[2] as f32 * alpha).min(255.0) as u8;
                    }
                }
            }
        }
    }
}

fn generate_image(seed: u32, output_path: &Path) {
    println!("Generating river network with seed {}...", seed);

    let terrain_size = 1024;
    let image_size = 1920;
    let flow_threshold = 12.0;

    let config = TerrainConfig {
        width: terrain_size,
        height: terrain_size,
        seed,
        ..Default::default()
    };
    let terrain = Terrain::generate(config);
    let hydrology = HydrologyData::simulate(&terrain, flow_threshold);

    let outlets = find_major_outlets(&hydrology, terrain_size, terrain_size);
    println!("  {} regions, {} river segments", outlets.len(), hydrology.river_segments.len());

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

    println!("\nDone! Generated {} images in the 'images' directory.", seeds.len() + 1);
}
