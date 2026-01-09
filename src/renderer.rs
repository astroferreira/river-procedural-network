//! GPU-accelerated renderer using wgpu
//! Renders beautiful river networks with bloom and glow effects

use crate::hydrology::{HydrologyData, RiverSegment};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use winit::window::Window;

/// Vertex data for river segments
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RiverVertex {
    pub position: [f32; 2],
    pub flow: f32,
    pub stream_order: f32,
    pub watershed_id: f32,
    pub progress: f32, // 0.0 = start, 1.0 = end
}

/// Uniform buffer for camera and rendering parameters
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub resolution: [f32; 2],
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub time: f32,
    pub max_flow: f32,
    pub num_watersheds: f32,
}

/// Color palette entry
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PaletteColor {
    pub color: [f32; 4],
}

/// Main renderer state
pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    // River rendering pipeline
    pub river_pipeline: wgpu::RenderPipeline,
    pub river_vertex_buffer: wgpu::Buffer,
    pub river_vertex_count: u32,

    // Background rendering pipeline
    pub background_pipeline: wgpu::RenderPipeline,

    // Uniforms
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,

    // Color palette
    pub palette_buffer: wgpu::Buffer,
    pub palette_bind_group: wgpu::BindGroup,

    // Render state
    pub uniforms: Uniforms,
    pub scale: f32,
    pub offset: (f32, f32),
}

impl Renderer {
    /// Create a new renderer
    pub async fn new(window: &'static Window) -> Self {
        let size = window.inner_size();

        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create surface
        let surface = instance.create_surface(window).unwrap();

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find a suitable GPU adapter");

        // Request device
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("River Renderer Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        // Configure surface
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create shaders
        let river_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("River Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/river.wgsl").into()),
        });

        let background_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Background Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/background.wgsl").into()),
        });

        // Create uniform buffer
        let uniforms = Uniforms {
            resolution: [size.width as f32, size.height as f32],
            scale: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            time: 0.0,
            max_flow: 1.0,
            num_watersheds: 1.0,
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create uniform bind group layout
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create color palette buffer (256 colors max)
        let palette = Self::generate_watershed_palette(256);
        let palette_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Palette Buffer"),
            contents: bytemuck::cast_slice(&palette),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let palette_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Palette Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let palette_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Palette Bind Group"),
            layout: &palette_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: palette_buffer.as_entire_binding(),
            }],
        });

        // Create river pipeline layout
        let river_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("River Pipeline Layout"),
                bind_group_layouts: &[&uniform_bind_group_layout, &palette_bind_group_layout],
                push_constant_ranges: &[],
            });

        // River vertex buffer layout
        let river_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RiverVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 20,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };

        // Create river render pipeline
        let river_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("River Pipeline"),
            layout: Some(&river_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &river_shader,
                entry_point: "vs_main",
                buffers: &[river_vertex_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &river_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Create background pipeline
        let background_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Background Pipeline Layout"),
                bind_group_layouts: &[&uniform_bind_group_layout],
                push_constant_ranges: &[],
            });

        let background_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Background Pipeline"),
            layout: Some(&background_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &background_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &background_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // Create empty vertex buffer (will be populated later)
        let river_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("River Vertex Buffer"),
            size: 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            surface,
            device,
            queue,
            config,
            size,
            river_pipeline,
            river_vertex_buffer,
            river_vertex_count: 0,
            background_pipeline,
            uniform_buffer,
            uniform_bind_group,
            palette_buffer,
            palette_bind_group,
            uniforms,
            scale: 1.0,
            offset: (0.0, 0.0),
        }
    }

    /// Generate a beautiful color palette for watersheds
    fn generate_watershed_palette(count: usize) -> Vec<PaletteColor> {
        let mut palette = Vec::with_capacity(count);

        // Hand-picked beautiful colors inspired by the reference image
        let base_colors: Vec<[f32; 3]> = vec![
            [0.95, 0.2, 0.4],   // Bright red/pink
            [0.2, 0.8, 0.6],    // Teal/cyan
            [0.9, 0.7, 0.1],    // Gold/yellow
            [0.3, 0.5, 0.95],   // Blue
            [0.8, 0.3, 0.8],    // Magenta/purple
            [0.1, 0.9, 0.4],    // Bright green
            [0.95, 0.5, 0.2],   // Orange
            [0.4, 0.9, 0.9],    // Cyan
            [0.9, 0.4, 0.6],    // Pink
            [0.6, 0.8, 0.2],    // Lime
            [0.5, 0.3, 0.9],    // Purple
            [0.2, 0.7, 0.8],    // Sky blue
            [0.9, 0.6, 0.5],    // Salmon
            [0.3, 0.9, 0.7],    // Mint
            [0.8, 0.8, 0.3],    // Yellow-green
            [0.7, 0.4, 0.7],    // Lavender
        ];

        for i in 0..count {
            let base_idx = i % base_colors.len();
            let variation = (i / base_colors.len()) as f32 * 0.1;

            let base = base_colors[base_idx];
            let color = [
                (base[0] + variation * 0.5).clamp(0.0, 1.0),
                (base[1] - variation * 0.3).clamp(0.0, 1.0),
                (base[2] + variation * 0.2).clamp(0.0, 1.0),
                1.0,
            ];

            palette.push(PaletteColor { color });
        }

        palette
    }

    /// Update river data for rendering
    pub fn update_river_data(&mut self, hydrology: &HydrologyData, terrain_width: usize, terrain_height: usize) {
        let vertices = Self::create_river_vertices(
            &hydrology.river_segments,
            terrain_width,
            terrain_height,
        );

        self.river_vertex_count = vertices.len() as u32;

        if !vertices.is_empty() {
            // Create new buffer with appropriate size
            self.river_vertex_buffer = self.device.create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: Some("River Vertex Buffer"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }
            );
        }

        // Update uniforms
        let max_flow = hydrology.river_segments
            .iter()
            .map(|s| s.flow)
            .fold(0.0f32, |a, b| a.max(b));

        self.uniforms.max_flow = max_flow;
        self.uniforms.num_watersheds = hydrology.num_watersheds as f32;

        log::info!("Updated river data: {} vertices, {} watersheds, max_flow: {}",
            self.river_vertex_count, hydrology.num_watersheds, max_flow);
    }

    /// Create vertices for river segments (as thick lines using triangles)
    fn create_river_vertices(
        segments: &[RiverSegment],
        terrain_width: usize,
        terrain_height: usize,
    ) -> Vec<RiverVertex> {
        let mut vertices = Vec::with_capacity(segments.len() * 6);

        let tw = terrain_width as f32;
        let th = terrain_height as f32;

        for segment in segments {
            // Normalize coordinates to [-1, 1]
            let x0 = (segment.start.0 / tw) * 2.0 - 1.0;
            let y0 = (segment.start.1 / th) * 2.0 - 1.0;
            let x1 = (segment.end.0 / tw) * 2.0 - 1.0;
            let y1 = (segment.end.1 / th) * 2.0 - 1.0;

            // Calculate perpendicular for line thickness
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len = (dx * dx + dy * dy).sqrt();

            if len < 0.0001 {
                continue;
            }

            // Base width scales with flow (log scale for visual appeal)
            let base_width = 0.0008 + segment.flow * 0.0004;
            let width = base_width.min(0.008);

            let nx = -dy / len * width;
            let ny = dx / len * width;

            // Create quad as two triangles
            let v0 = RiverVertex {
                position: [x0 - nx, y0 - ny],
                flow: segment.flow,
                stream_order: segment.stream_order as f32,
                watershed_id: segment.watershed_id as f32,
                progress: 0.0,
            };
            let v1 = RiverVertex {
                position: [x0 + nx, y0 + ny],
                flow: segment.flow,
                stream_order: segment.stream_order as f32,
                watershed_id: segment.watershed_id as f32,
                progress: 0.0,
            };
            let v2 = RiverVertex {
                position: [x1 - nx, y1 - ny],
                flow: segment.flow,
                stream_order: segment.stream_order as f32,
                watershed_id: segment.watershed_id as f32,
                progress: 1.0,
            };
            let v3 = RiverVertex {
                position: [x1 + nx, y1 + ny],
                flow: segment.flow,
                stream_order: segment.stream_order as f32,
                watershed_id: segment.watershed_id as f32,
                progress: 1.0,
            };

            // Triangle 1
            vertices.push(v0);
            vertices.push(v1);
            vertices.push(v2);

            // Triangle 2
            vertices.push(v1);
            vertices.push(v3);
            vertices.push(v2);
        }

        vertices
    }

    /// Resize the renderer
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            self.uniforms.resolution = [new_size.width as f32, new_size.height as f32];
        }
    }

    /// Update camera/view state
    pub fn update_view(&mut self, scale: f32, offset: (f32, f32)) {
        self.scale = scale;
        self.offset = offset;
        self.uniforms.scale = scale;
        self.uniforms.offset_x = offset.0;
        self.uniforms.offset_y = offset.1;
    }

    /// Update time for animations
    pub fn update_time(&mut self, time: f32) {
        self.uniforms.time = time;
    }

    /// Render a frame
    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Update uniform buffer
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Draw background
            render_pass.set_pipeline(&self.background_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.draw(0..6, 0..1);

            // Draw rivers
            if self.river_vertex_count > 0 {
                render_pass.set_pipeline(&self.river_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_bind_group(1, &self.palette_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.river_vertex_buffer.slice(..));
                render_pass.draw(0..self.river_vertex_count, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
