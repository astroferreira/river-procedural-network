//! Procedural River Network Generator
//!
//! A beautiful visualization of procedurally generated river networks
//! inspired by watershed maps. Features GPU-accelerated rendering with
//! realistic hydrology simulation using particle-based hydraulic erosion.
//!
//! Based on Nick McDonald's procedural hydrology:
//! https://nickmcd.me/2020/04/15/procedural-hydrology/
//!
//! Controls:
//! - Mouse drag: Pan the view
//! - Mouse wheel: Zoom in/out
//! - Space: Generate new random terrain
//! - R: Reset view
//! - +/-: Adjust flow threshold
//! - E: Toggle erosion mode (particle-based vs D8)
//! - [/]: Adjust erosion iterations
//! - Escape: Quit

mod terrain;
mod hydrology;
mod renderer;
mod particle;

use terrain::{Terrain, TerrainConfig};
use hydrology::HydrologyData;
use renderer::Renderer;
use particle::ErosionParams;

use std::time::Instant;
use winit::{
    event::{Event, WindowEvent, ElementState, MouseButton, MouseScrollDelta},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

/// Application state
struct App {
    renderer: Renderer,
    terrain: Option<Terrain>,
    hydrology: Option<HydrologyData>,

    // View state
    scale: f32,
    offset: (f32, f32),
    dragging: bool,
    last_mouse_pos: (f32, f32),

    // Generation parameters
    terrain_size: usize,
    flow_threshold: f32,
    seed: u32,

    // Erosion parameters (particle-based simulation)
    use_erosion: bool,
    erosion_params: ErosionParams,

    // Animation
    start_time: Instant,
}

impl App {
    /// Generate new terrain and river network
    fn generate(&mut self) {
        if self.use_erosion {
            log::info!("Generating river network with particle erosion (seed {}, {} iterations)...",
                self.seed, self.erosion_params.iterations);
        } else {
            log::info!("Generating river network with D8 flow (seed {})...", self.seed);
        }

        let config = TerrainConfig {
            width: self.terrain_size,
            height: self.terrain_size,
            seed: self.seed,
            ..Default::default()
        };

        let terrain = Terrain::generate(config);

        let hydrology = if self.use_erosion {
            HydrologyData::simulate_with_erosion(
                &terrain,
                self.flow_threshold,
                &self.erosion_params,
                self.seed,
            )
        } else {
            HydrologyData::simulate(&terrain, self.flow_threshold)
        };

        self.renderer.update_river_data(&hydrology, terrain.width, terrain.height);

        self.terrain = Some(terrain);
        self.hydrology = Some(hydrology);

        log::info!("Generation complete!");
    }

    /// Toggle between erosion and D8 mode
    fn toggle_erosion(&mut self) {
        self.use_erosion = !self.use_erosion;
        if self.use_erosion {
            log::info!("Switched to PARTICLE EROSION mode ({} iterations)",
                self.erosion_params.iterations);
        } else {
            log::info!("Switched to D8 FLOW DIRECTION mode");
        }
        self.generate();
    }

    /// Adjust erosion iterations
    fn adjust_iterations(&mut self, factor: f32) {
        let new_iters = ((self.erosion_params.iterations as f32) * factor) as usize;
        self.erosion_params.iterations = new_iters.clamp(10_000, 1_000_000);
        log::info!("Erosion iterations: {}", self.erosion_params.iterations);
        if self.use_erosion {
            self.generate();
        }
    }

    /// Reset view to default
    fn reset_view(&mut self) {
        self.scale = 1.0;
        self.offset = (0.0, 0.0);
        self.renderer.update_view(self.scale, self.offset);
    }

    /// Update flow threshold and regenerate rivers
    fn adjust_threshold(&mut self, delta: f32) {
        self.flow_threshold = (self.flow_threshold + delta).max(10.0).min(1000.0);
        log::info!("Flow threshold: {}", self.flow_threshold);

        // Regenerate with new threshold
        if let Some(ref terrain) = self.terrain {
            let hydrology = HydrologyData::simulate(terrain, self.flow_threshold);
            self.renderer.update_river_data(&hydrology, terrain.width, terrain.height);
            self.hydrology = Some(hydrology);
        }
    }
}

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    log::info!("╔═══════════════════════════════════════════════════════════════╗");
    log::info!("║     Procedural River Network Generator                        ║");
    log::info!("║     Based on Nick McDonald's Procedural Hydrology             ║");
    log::info!("╠═══════════════════════════════════════════════════════════════╣");
    log::info!("║  Controls:                                                    ║");
    log::info!("║    Mouse drag  - Pan the view                                 ║");
    log::info!("║    Mouse wheel - Zoom in/out                                  ║");
    log::info!("║    Space       - Generate new random terrain                  ║");
    log::info!("║    R           - Reset view                                   ║");
    log::info!("║    +/-         - Adjust river detail (flow threshold)         ║");
    log::info!("║    E           - Toggle erosion mode (particle vs D8)         ║");
    log::info!("║    [/]         - Decrease/increase erosion iterations         ║");
    log::info!("║    S/A         - Increase/decrease terrain size               ║");
    log::info!("║    Escape      - Quit                                         ║");
    log::info!("╚═══════════════════════════════════════════════════════════════╝");

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let window = WindowBuilder::new()
        .with_title("Procedural River Network Generator")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 960))
        .build(&event_loop)
        .unwrap();

    // Leak window to get static lifetime (necessary for wgpu surface)
    let window: &'static winit::window::Window = Box::leak(Box::new(window));

    // Initialize renderer
    let renderer = pollster::block_on(Renderer::new(window));

    let mut app = App {
        renderer,
        terrain: None,
        hydrology: None,
        scale: 1.0,
        offset: (0.0, 0.0),
        dragging: false,
        last_mouse_pos: (0.0, 0.0),
        terrain_size: 512,  // Smaller default for faster erosion simulation
        flow_threshold: 100.0,
        seed: 42,
        use_erosion: true,  // Default to particle-based erosion
        erosion_params: ErosionParams::default(),
        start_time: Instant::now(),
    };

    // Generate initial terrain
    app.generate();

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    elwt.exit();
                }

                WindowEvent::Resized(physical_size) => {
                    app.renderer.resize(physical_size);
                }

                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        match event.physical_key {
                            PhysicalKey::Code(KeyCode::Escape) => elwt.exit(),
                            PhysicalKey::Code(KeyCode::Space) => {
                                // Generate new random terrain
                                app.seed = rand::random();
                                app.generate();
                            }
                            PhysicalKey::Code(KeyCode::KeyR) => app.reset_view(),
                            PhysicalKey::Code(KeyCode::Equal) |
                            PhysicalKey::Code(KeyCode::NumpadAdd) => app.adjust_threshold(-20.0),
                            PhysicalKey::Code(KeyCode::Minus) |
                            PhysicalKey::Code(KeyCode::NumpadSubtract) => app.adjust_threshold(20.0),
                            PhysicalKey::Code(KeyCode::KeyE) => app.toggle_erosion(),
                            PhysicalKey::Code(KeyCode::BracketLeft) => app.adjust_iterations(0.5),
                            PhysicalKey::Code(KeyCode::BracketRight) => app.adjust_iterations(2.0),
                            PhysicalKey::Code(KeyCode::KeyS) => {
                                // Increase terrain size
                                app.terrain_size = (app.terrain_size + 256).min(2048);
                                log::info!("Terrain size: {}", app.terrain_size);
                                app.generate();
                            }
                            PhysicalKey::Code(KeyCode::KeyA) => {
                                // Decrease terrain size
                                app.terrain_size = (app.terrain_size - 256).max(256);
                                log::info!("Terrain size: {}", app.terrain_size);
                                app.generate();
                            }
                            _ => {}
                        }
                    }
                }

                WindowEvent::MouseInput { state, button, .. } => {
                    if button == MouseButton::Left {
                        app.dragging = state == ElementState::Pressed;
                    }
                }

                WindowEvent::CursorMoved { position, .. } => {
                    let x = position.x as f32;
                    let y = position.y as f32;

                    if app.dragging {
                        let dx = (x - app.last_mouse_pos.0) / app.renderer.size.width as f32 * 2.0;
                        let dy = (y - app.last_mouse_pos.1) / app.renderer.size.height as f32 * 2.0;

                        app.offset.0 += dx / app.scale;
                        app.offset.1 -= dy / app.scale;

                        app.renderer.update_view(app.scale, app.offset);
                    }

                    app.last_mouse_pos = (x, y);
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 100.0,
                    };

                    // Zoom centered on mouse position
                    let zoom_factor = 1.1f32.powf(scroll);
                    app.scale *= zoom_factor;
                    app.scale = app.scale.clamp(0.1, 10.0);

                    app.renderer.update_view(app.scale, app.offset);
                }

                WindowEvent::RedrawRequested => {
                    // Update time for animations
                    let elapsed = app.start_time.elapsed().as_secs_f32();
                    app.renderer.update_time(elapsed);

                    match app.renderer.render() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            app.renderer.resize(app.renderer.size);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            elwt.exit();
                        }
                        Err(e) => {
                            eprintln!("Render error: {:?}", e);
                        }
                    }
                }

                _ => {}
            },

            Event::AboutToWait => {
                window.request_redraw();
            }

            _ => {}
        }
    }).unwrap();
}
