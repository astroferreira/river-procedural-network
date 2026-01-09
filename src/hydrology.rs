//! Hydrology simulation for river network generation
//! Implements D8 flow direction, flow accumulation, and river extraction

use crate::terrain::Terrain;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rayon::prelude::*;
use std::collections::VecDeque;

/// D8 flow directions (8 cardinal + diagonal directions)
/// Encoded as bit flags: N=1, NE=2, E=4, SE=8, S=16, SW=32, W=64, NW=128
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FlowDirection {
    None = 0,
    North = 1,
    NorthEast = 2,
    East = 4,
    SouthEast = 8,
    South = 16,
    SouthWest = 32,
    West = 64,
    NorthWest = 128,
}

impl FlowDirection {
    /// Get the dx, dy offset for this direction
    pub fn offset(&self) -> (i32, i32) {
        match self {
            FlowDirection::None => (0, 0),
            FlowDirection::North => (0, -1),
            FlowDirection::NorthEast => (1, -1),
            FlowDirection::East => (1, 0),
            FlowDirection::SouthEast => (1, 1),
            FlowDirection::South => (0, 1),
            FlowDirection::SouthWest => (-1, 1),
            FlowDirection::West => (-1, 0),
            FlowDirection::NorthWest => (-1, -1),
        }
    }

    /// Get all 8 directions
    pub fn all() -> [FlowDirection; 8] {
        [
            FlowDirection::North,
            FlowDirection::NorthEast,
            FlowDirection::East,
            FlowDirection::SouthEast,
            FlowDirection::South,
            FlowDirection::SouthWest,
            FlowDirection::West,
            FlowDirection::NorthWest,
        ]
    }

    /// Distance factor for diagonal vs cardinal
    pub fn distance_factor(&self) -> f32 {
        match self {
            FlowDirection::NorthEast |
            FlowDirection::SouthEast |
            FlowDirection::SouthWest |
            FlowDirection::NorthWest => std::f32::consts::SQRT_2,
            _ => 1.0,
        }
    }
}

/// River segment for rendering
#[derive(Clone, Debug)]
pub struct RiverSegment {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub flow: f32,
    pub stream_order: u8,
    pub watershed_id: u32,
}

/// Hydrology simulation result
pub struct HydrologyData {
    pub width: usize,
    pub height: usize,
    pub flow_direction: Vec<FlowDirection>,
    pub flow_accumulation: Vec<f32>,
    pub stream_order: Vec<u8>,
    pub watershed_id: Vec<u32>,
    pub river_segments: Vec<RiverSegment>,
    pub num_watersheds: u32,
}

impl HydrologyData {
    /// Run full hydrology simulation on terrain with discrete source points
    pub fn simulate(terrain: &Terrain, flow_threshold: f32) -> Self {
        Self::simulate_with_sources(terrain, flow_threshold, 42, 200)
    }

    /// Run hydrology simulation with specified number of random source points
    /// Uses hybrid model: discrete main sources + natural tributary formation
    pub fn simulate_with_sources(
        terrain: &Terrain,
        flow_threshold: f32,
        seed: u32,
        num_sources: usize,
    ) -> Self {
        log::info!("Starting hydrology simulation with {} source points...", num_sources);

        let width = terrain.width;
        let height = terrain.height;

        // Step 1: Calculate flow directions using D8 algorithm
        log::info!("Calculating flow directions...");
        let flow_direction = Self::calculate_flow_directions(terrain);

        // Step 2: Calculate FULL flow accumulation (capillary model - every cell contributes)
        log::info!("Calculating flow accumulation...");
        let mut flow_accumulation = Self::calculate_flow_accumulation(&flow_direction, width, height);

        // Step 3: Generate discrete source points and boost their flow paths
        log::info!("Generating {} source points...", num_sources);
        let sources = Self::generate_source_points(terrain, &flow_direction, seed, num_sources, width, height);

        // Boost flow along main river paths from sources
        Self::boost_source_paths(&flow_direction, &sources, &mut flow_accumulation, width, height);

        // Step 4: For single source, create a mask of connected watershed
        let river_mask = if num_sources == 1 && !sources.is_empty() {
            log::info!("Creating single river watershed mask...");
            Some(Self::create_single_river_mask(&flow_direction, &sources[0], width, height))
        } else {
            None
        };

        // Step 5: Identify watersheds
        log::info!("Delineating watersheds...");
        let (watershed_id, num_watersheds) = Self::delineate_watersheds(
            &flow_direction, &flow_accumulation, width, height, flow_threshold
        );

        // Step 6: Calculate Strahler stream order
        log::info!("Calculating stream orders...");
        let stream_order = Self::calculate_stream_order(
            &flow_direction, &flow_accumulation, width, height, flow_threshold
        );

        // Step 7: Extract river segments for rendering
        log::info!("Extracting river segments...");
        let river_segments = Self::extract_river_segments_masked(
            &flow_direction, &flow_accumulation, &stream_order, &watershed_id,
            width, height, flow_threshold, river_mask.as_deref()
        );

        log::info!("Hydrology simulation complete: {} sources, {} river segments",
            num_sources, river_segments.len());

        Self {
            width,
            height,
            flow_direction,
            flow_accumulation,
            stream_order,
            watershed_id,
            river_segments,
            num_watersheds,
        }
    }

    /// Create a mask of all cells that drain into the single source's river path
    fn create_single_river_mask(
        flow_direction: &[FlowDirection],
        source: &(usize, usize, f32),
        width: usize,
        height: usize,
    ) -> Vec<bool> {
        let mut mask = vec![false; width * height];
        let (sx, sy, _) = *source;

        // First, mark the main river path
        let mut path_cells = Vec::new();
        let mut x = sx;
        let mut y = sy;
        let max_steps = width + height;
        let mut steps = 0;

        while steps < max_steps {
            let idx = y * width + x;
            mask[idx] = true;
            path_cells.push((x, y));

            let dir = flow_direction[idx];
            if dir == FlowDirection::None {
                break;
            }

            let (dx, dy) = dir.offset();
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;

            if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                break;
            }

            x = nx as usize;
            y = ny as usize;
            steps += 1;
        }

        // Then, for each cell on the path, trace upstream to find all tributaries
        for (px, py) in path_cells {
            Self::mark_upstream_tributaries(flow_direction, &mut mask, px, py, width, height);
        }

        mask
    }

    /// Recursively mark all cells that flow into the given cell
    fn mark_upstream_tributaries(
        flow_direction: &[FlowDirection],
        mask: &mut [bool],
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) {
        let mut stack = vec![(x, y)];

        while let Some((cx, cy)) = stack.pop() {
            // Find all neighbors that flow INTO this cell
            for dir in FlowDirection::all() {
                let (dx, dy) = dir.offset();
                let nx = cx as i32 - dx; // Reverse direction
                let ny = cy as i32 - dy;

                if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                    let nidx = ny as usize * width + nx as usize;

                    if !mask[nidx] {
                        let neighbor_dir = flow_direction[nidx];
                        let (ndx, ndy) = neighbor_dir.offset();

                        // Check if this neighbor flows into current cell
                        if nx + ndx as i32 == cx as i32 && ny + ndy as i32 == cy as i32 {
                            mask[nidx] = true;
                            stack.push((nx as usize, ny as usize));
                        }
                    }
                }
            }
        }
    }

    /// Generate random source points - spread across the map with some highland bias
    fn generate_source_points(
        terrain: &Terrain,
        flow_direction: &[FlowDirection],
        seed: u32,
        num_sources: usize,
        width: usize,
        height: usize,
    ) -> Vec<(usize, usize, f32)> {
        let mut rng = StdRng::seed_from_u64(seed as u64);

        // For single source, find the BEST candidate (longest path from high terrain)
        if num_sources == 1 {
            return Self::find_best_single_source(terrain, flow_direction, seed, width, height);
        }

        // Margin from edges - sources start inland
        let margin = 50;

        let mut sources = Vec::with_capacity(num_sources);
        let mut attempts = 0;
        let max_attempts = num_sources * 200;

        while sources.len() < num_sources && attempts < max_attempts {
            attempts += 1;

            let x = rng.gen_range(margin..width - margin);
            let y = rng.gen_range(margin..height - margin);

            let idx = y * width + x;
            let dir = flow_direction[idx];

            if dir == FlowDirection::None {
                continue;
            }

            // Require LONG paths - at least 100 cells to edge for longer rivers
            let path_len = Self::trace_path_length(flow_direction, x, y, width, height);
            if path_len < 100 {
                continue;
            }

            let h = terrain.get_height(x, y);

            // Highland bias - prefer higher terrain for source points
            let threshold = rng.gen::<f32>() * 0.35;
            if h < threshold {
                continue;
            }

            // Large minimum distance between sources for sparser coverage
            let min_dist = (width.min(height) / 10) as f32;
            let too_close = sources.iter().any(|(sx, sy, _): &(usize, usize, f32)| {
                let dx = x as f32 - *sx as f32;
                let dy = y as f32 - *sy as f32;
                (dx * dx + dy * dy).sqrt() < min_dist
            });

            if !too_close {
                // Higher source strength for more prominent rivers
                let flow_strength = 100.0 + rng.gen::<f32>() * 300.0;
                sources.push((x, y, flow_strength));
            }
        }

        sources
    }

    /// Find the best single source point - longest path from highest terrain
    fn find_best_single_source(
        terrain: &Terrain,
        flow_direction: &[FlowDirection],
        seed: u32,
        width: usize,
        height: usize,
    ) -> Vec<(usize, usize, f32)> {
        let mut rng = StdRng::seed_from_u64(seed as u64);
        let margin = 100; // Start well inland

        let mut best_source: Option<(usize, usize, usize, f32)> = None; // x, y, path_len, height

        // Sample many candidates and pick the best one
        for _ in 0..5000 {
            let x = rng.gen_range(margin..width - margin);
            let y = rng.gen_range(margin..height - margin);

            let idx = y * width + x;
            let dir = flow_direction[idx];

            if dir == FlowDirection::None {
                continue;
            }

            let path_len = Self::trace_path_length(flow_direction, x, y, width, height);
            let h = terrain.get_height(x, y);

            // Score based on path length and height (prefer long paths from high terrain)
            let score = path_len as f32 * (0.5 + h);

            if let Some((_, _, best_len, best_h)) = best_source {
                let best_score = best_len as f32 * (0.5 + best_h);
                if score > best_score {
                    best_source = Some((x, y, path_len, h));
                }
            } else if path_len > 50 {
                best_source = Some((x, y, path_len, h));
            }
        }

        if let Some((x, y, _, _)) = best_source {
            // Very strong boost for single river to make it prominent with tendrils
            vec![(x, y, 500.0)]
        } else {
            vec![]
        }
    }

    /// Trace path length from a point to edge/pit
    fn trace_path_length(
        flow_direction: &[FlowDirection],
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
    ) -> usize {
        let mut x = start_x;
        let mut y = start_y;
        let mut steps = 0;
        let max_steps = width + height;

        while steps < max_steps {
            let idx = y * width + x;
            let dir = flow_direction[idx];

            if dir == FlowDirection::None {
                break;
            }

            let (dx, dy) = dir.offset();
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;

            if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                break;
            }

            x = nx as usize;
            y = ny as usize;
            steps += 1;
        }

        steps
    }

    /// Boost flow along paths from discrete source points
    /// This creates prominent main rivers while keeping natural tributaries
    fn boost_source_paths(
        flow_direction: &[FlowDirection],
        sources: &[(usize, usize, f32)],
        accumulation: &mut [f32],
        width: usize,
        height: usize,
    ) {
        for (sx, sy, strength) in sources {
            let mut x = *sx;
            let mut y = *sy;
            let mut steps = 0;
            let max_steps = width + height;

            while steps < max_steps {
                let idx = y * width + x;
                accumulation[idx] += *strength;

                let dir = flow_direction[idx];
                if dir == FlowDirection::None {
                    break;
                }

                let (dx, dy) = dir.offset();
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;

                if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                    break;
                }

                x = nx as usize;
                y = ny as usize;
                steps += 1;
            }
        }
    }

    /// D8 flow direction algorithm
    fn calculate_flow_directions(terrain: &Terrain) -> Vec<FlowDirection> {
        let width = terrain.width;
        let height = terrain.height;

        (0..height)
            .into_par_iter()
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let current_height = terrain.get_height(x, y);
                    let mut steepest_dir = FlowDirection::None;
                    let mut steepest_slope = 0.0f32;

                    for dir in FlowDirection::all() {
                        let (dx, dy) = dir.offset();
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;

                        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                            let neighbor_height = terrain.get_height(nx as usize, ny as usize);
                            let drop = current_height - neighbor_height;
                            let slope = drop / dir.distance_factor();

                            if slope > steepest_slope {
                                steepest_slope = slope;
                                steepest_dir = dir;
                            }
                        }
                    }

                    // If no downhill neighbor, flow to edge (pour point)
                    if steepest_dir == FlowDirection::None && (x == 0 || x == width - 1 || y == 0 || y == height - 1) {
                        // At edge, mark as pour point
                        steepest_dir = FlowDirection::None;
                    }

                    steepest_dir
                }).collect::<Vec<FlowDirection>>()
            })
            .collect()
    }

    /// Calculate flow accumulation using upstream area counting
    fn calculate_flow_accumulation(
        flow_direction: &[FlowDirection],
        width: usize,
        height: usize,
    ) -> Vec<f32> {
        let mut accumulation = vec![1.0f32; width * height];
        let mut in_degree = vec![0u32; width * height];

        // Calculate in-degree for each cell (how many cells flow into it)
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let dir = flow_direction[idx];
                if dir != FlowDirection::None {
                    let (dx, dy) = dir.offset();
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;

                    if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                        let nidx = ny as usize * width + nx as usize;
                        in_degree[nidx] += 1;
                    }
                }
            }
        }

        // Use topological sort to accumulate flow
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();

        // Start with cells that have no upstream neighbors
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if in_degree[idx] == 0 {
                    queue.push_back((x, y));
                }
            }
        }

        while let Some((x, y)) = queue.pop_front() {
            let idx = y * width + x;
            let dir = flow_direction[idx];

            if dir != FlowDirection::None {
                let (dx, dy) = dir.offset();
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;

                if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                    let nidx = ny as usize * width + nx as usize;
                    accumulation[nidx] += accumulation[idx];
                    in_degree[nidx] -= 1;

                    if in_degree[nidx] == 0 {
                        queue.push_back((nx as usize, ny as usize));
                    }
                }
            }
        }

        accumulation
    }

    /// Delineate watersheds by tracing from pour points
    fn delineate_watersheds(
        flow_direction: &[FlowDirection],
        flow_accumulation: &[f32],
        width: usize,
        height: usize,
        flow_threshold: f32,
    ) -> (Vec<u32>, u32) {
        let mut watershed_id = vec![0u32; width * height];
        let mut current_id = 0u32;

        // Find pour points (edges with high flow accumulation)
        // Use a higher threshold to identify only MAJOR drainage basins
        let major_threshold = flow_threshold * 50.0;
        let mut pour_points: Vec<(usize, usize, f32)> = Vec::new();

        for x in 0..width {
            // Top and bottom edges
            for &y in &[0, height - 1] {
                let idx = y * width + x;
                let flow = flow_accumulation[idx];
                if flow > major_threshold {
                    pour_points.push((x, y, flow));
                }
            }
        }

        for y in 1..height - 1 {
            // Left and right edges
            for &x in &[0, width - 1] {
                let idx = y * width + x;
                let flow = flow_accumulation[idx];
                if flow > major_threshold {
                    pour_points.push((x, y, flow));
                }
            }
        }

        // Sort pour points by flow (largest first)
        pour_points.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

        // Limit to top N watersheds for cleaner visualization
        let max_watersheds = 30;
        pour_points.truncate(max_watersheds);

        // Trace upstream from each pour point to delineate watershed
        for (px, py, _) in &pour_points {
            let idx = py * width + px;
            if watershed_id[idx] == 0 {
                current_id += 1;
                Self::trace_watershed_upstream(
                    flow_direction, &mut watershed_id,
                    width, height, *px, *py, current_id
                );
            }
        }

        // For remaining unassigned cells, trace downstream until we hit
        // an assigned watershed, then assign to that watershed
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if watershed_id[idx] == 0 {
                    // Trace downstream to find which watershed this drains to
                    let target_id = Self::trace_downstream_to_watershed(
                        flow_direction, &watershed_id,
                        width, height, x, y
                    );

                    if target_id > 0 {
                        // Assign this cell and all upstream cells to the target watershed
                        Self::trace_watershed_upstream(
                            flow_direction, &mut watershed_id,
                            width, height, x, y, target_id
                        );
                    } else {
                        // No existing watershed found, create a new small one
                        current_id += 1;
                        Self::trace_watershed_upstream(
                            flow_direction, &mut watershed_id,
                            width, height, x, y, current_id
                        );
                    }
                }
            }
        }

        (watershed_id, current_id)
    }

    /// Trace downstream from a cell to find which watershed it drains to
    fn trace_downstream_to_watershed(
        flow_direction: &[FlowDirection],
        watershed_id: &[u32],
        width: usize,
        height: usize,
        start_x: usize,
        start_y: usize,
    ) -> u32 {
        let mut x = start_x;
        let mut y = start_y;
        let mut steps = 0;
        let max_steps = width * height; // Prevent infinite loops

        while steps < max_steps {
            let idx = y * width + x;

            // If we hit an assigned cell, return its watershed
            if watershed_id[idx] > 0 {
                return watershed_id[idx];
            }

            let dir = flow_direction[idx];
            if dir == FlowDirection::None {
                // Reached edge or pit
                return 0;
            }

            let (dx, dy) = dir.offset();
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;

            if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                // Flowed off the edge
                return 0;
            }

            x = nx as usize;
            y = ny as usize;
            steps += 1;
        }

        0
    }

    /// Trace upstream to assign watershed ID
    fn trace_watershed_upstream(
        flow_direction: &[FlowDirection],
        watershed_id: &mut [u32],
        width: usize,
        height: usize,
        start_x: usize,
        start_y: usize,
        id: u32,
    ) {
        let mut stack = vec![(start_x, start_y)];

        while let Some((x, y)) = stack.pop() {
            let idx = y * width + x;
            if watershed_id[idx] != 0 {
                continue;
            }

            watershed_id[idx] = id;

            // Find all cells that flow into this cell
            for dir in FlowDirection::all() {
                let (dx, dy) = dir.offset();
                let nx = x as i32 - dx; // Reverse direction to go upstream
                let ny = y as i32 - dy;

                if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                    let nidx = ny as usize * width + nx as usize;
                    let neighbor_dir = flow_direction[nidx];

                    // Check if neighbor flows into current cell
                    let (ndx, ndy) = neighbor_dir.offset();
                    if nx + ndx as i32 == x as i32 && ny + ndy as i32 == y as i32 {
                        if watershed_id[nidx] == 0 {
                            stack.push((nx as usize, ny as usize));
                        }
                    }
                }
            }
        }
    }

    /// Calculate Strahler stream order
    fn calculate_stream_order(
        flow_direction: &[FlowDirection],
        flow_accumulation: &[f32],
        width: usize,
        height: usize,
        flow_threshold: f32,
    ) -> Vec<u8> {
        let mut stream_order = vec![0u8; width * height];

        // Initialize: cells above threshold start with order 1
        for i in 0..flow_accumulation.len() {
            if flow_accumulation[i] >= flow_threshold {
                stream_order[i] = 1;
            }
        }

        // Iteratively update stream orders
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: u32 = 20;

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;

            for y in 0..height {
                for x in 0..width {
                    let idx = y * width + x;
                    if stream_order[idx] == 0 {
                        continue;
                    }

                    // Find upstream tributaries
                    let mut upstream_orders: Vec<u8> = Vec::new();

                    for dir in FlowDirection::all() {
                        let (dx, dy) = dir.offset();
                        let nx = x as i32 - dx;
                        let ny = y as i32 - dy;

                        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                            let nidx = ny as usize * width + nx as usize;
                            let neighbor_dir = flow_direction[nidx];

                            // Check if neighbor flows into current cell
                            let (ndx, ndy) = neighbor_dir.offset();
                            if nx + ndx as i32 == x as i32 && ny + ndy as i32 == y as i32 {
                                if stream_order[nidx] > 0 {
                                    upstream_orders.push(stream_order[nidx]);
                                }
                            }
                        }
                    }

                    if !upstream_orders.is_empty() {
                        upstream_orders.sort_unstable();
                        upstream_orders.reverse();

                        let new_order = if upstream_orders.len() >= 2 && upstream_orders[0] == upstream_orders[1] {
                            // Two streams of same order join -> increment order
                            upstream_orders[0].saturating_add(1)
                        } else {
                            // Keep highest upstream order
                            upstream_orders[0]
                        };

                        if new_order > stream_order[idx] {
                            stream_order[idx] = new_order;
                            changed = true;
                        }
                    }
                }
            }
        }

        stream_order
    }

    /// Extract river segments for rendering (with optional mask)
    fn extract_river_segments_masked(
        flow_direction: &[FlowDirection],
        flow_accumulation: &[f32],
        stream_order: &[u8],
        watershed_id: &[u32],
        width: usize,
        height: usize,
        flow_threshold: f32,
        mask: Option<&[bool]>,
    ) -> Vec<RiverSegment> {
        let mut segments = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;

                // Skip if masked out
                if let Some(m) = mask {
                    if !m[idx] {
                        continue;
                    }
                }

                let flow = flow_accumulation[idx];

                if flow >= flow_threshold {
                    let dir = flow_direction[idx];
                    if dir != FlowDirection::None {
                        let (dx, dy) = dir.offset();
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;

                        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                            segments.push(RiverSegment {
                                start: (x as f32, y as f32),
                                end: (nx as f32, ny as f32),
                                flow,  // Raw flow value - rendering will handle normalization
                                stream_order: stream_order[idx],
                                watershed_id: watershed_id[idx],
                            });
                        }
                    }
                }
            }
        }

        segments
    }

    /// Get flow accumulation at a point
    #[inline]
    pub fn get_flow(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.flow_accumulation[y * self.width + x]
        } else {
            0.0
        }
    }
}
