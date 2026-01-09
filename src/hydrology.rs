//! Hydrology simulation for river network generation
//! Implements D8 flow direction, flow accumulation, and river extraction

use crate::terrain::Terrain;
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
    /// Run full hydrology simulation on terrain
    pub fn simulate(terrain: &Terrain, flow_threshold: f32) -> Self {
        log::info!("Starting hydrology simulation...");

        let width = terrain.width;
        let height = terrain.height;

        // Step 1: Calculate flow directions using D8 algorithm
        log::info!("Calculating flow directions...");
        let flow_direction = Self::calculate_flow_directions(terrain);

        // Step 2: Calculate flow accumulation
        log::info!("Calculating flow accumulation...");
        let flow_accumulation = Self::calculate_flow_accumulation(&flow_direction, width, height);

        // Step 3: Identify watersheds using pour points at edges
        log::info!("Delineating watersheds...");
        let (watershed_id, num_watersheds) = Self::delineate_watersheds(
            &flow_direction, &flow_accumulation, width, height, flow_threshold
        );

        // Step 4: Calculate Strahler stream order
        log::info!("Calculating stream orders...");
        let stream_order = Self::calculate_stream_order(
            &flow_direction, &flow_accumulation, width, height, flow_threshold
        );

        // Step 5: Extract river segments for rendering
        log::info!("Extracting river segments...");
        let river_segments = Self::extract_river_segments(
            &flow_direction, &flow_accumulation, &stream_order, &watershed_id,
            width, height, flow_threshold
        );

        log::info!("Hydrology simulation complete: {} watersheds, {} river segments",
            num_watersheds, river_segments.len());

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
        let mut pour_points: Vec<(usize, usize, f32)> = Vec::new();

        for x in 0..width {
            // Top and bottom edges
            for &y in &[0, height - 1] {
                let idx = y * width + x;
                let flow = flow_accumulation[idx];
                if flow > flow_threshold * 10.0 {
                    pour_points.push((x, y, flow));
                }
            }
        }

        for y in 1..height - 1 {
            // Left and right edges
            for &x in &[0, width - 1] {
                let idx = y * width + x;
                let flow = flow_accumulation[idx];
                if flow > flow_threshold * 10.0 {
                    pour_points.push((x, y, flow));
                }
            }
        }

        // Sort pour points by flow (largest first)
        pour_points.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

        // Trace upstream from each pour point to delineate watershed
        for (px, py, _) in pour_points {
            let idx = py * width + px;
            if watershed_id[idx] == 0 {
                current_id += 1;
                Self::trace_watershed_upstream(
                    flow_direction, &mut watershed_id,
                    width, height, px, py, current_id
                );
            }
        }

        // Fill remaining unassigned cells
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if watershed_id[idx] == 0 {
                    current_id += 1;
                    Self::trace_watershed_upstream(
                        flow_direction, &mut watershed_id,
                        width, height, x, y, current_id
                    );
                }
            }
        }

        (watershed_id, current_id)
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

    /// Extract river segments for rendering
    fn extract_river_segments(
        flow_direction: &[FlowDirection],
        flow_accumulation: &[f32],
        stream_order: &[u8],
        watershed_id: &[u32],
        width: usize,
        height: usize,
        flow_threshold: f32,
    ) -> Vec<RiverSegment> {
        let mut segments = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
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
                                flow: flow.ln().max(0.0), // Log scale for better visualization
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
