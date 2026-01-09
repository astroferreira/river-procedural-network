// Terrain heightmap visualization shader
// Renders the eroded terrain with proper lighting and coloring

struct Uniforms {
    resolution: vec2<f32>,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    time: f32,
    max_flow: f32,
    num_watersheds: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var heightmap_texture: texture_2d<f32>;
@group(1) @binding(1) var heightmap_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Generate full-screen triangle (two triangles = 6 vertices)
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0)
    );

    let pos = positions[vertex_index];
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = pos * 0.5 + 0.5;

    return out;
}

// Color palette for terrain elevation
fn terrain_color(height: f32) -> vec3<f32> {
    // Dark depths to light highlands
    let deep_water = vec3<f32>(0.02, 0.04, 0.08);
    let shallow = vec3<f32>(0.03, 0.06, 0.10);
    let lowland = vec3<f32>(0.04, 0.08, 0.06);
    let midland = vec3<f32>(0.06, 0.10, 0.08);
    let highland = vec3<f32>(0.10, 0.14, 0.12);
    let peak = vec3<f32>(0.15, 0.18, 0.16);

    if (height < 0.1) {
        return mix(deep_water, shallow, height / 0.1);
    } else if (height < 0.3) {
        return mix(shallow, lowland, (height - 0.1) / 0.2);
    } else if (height < 0.5) {
        return mix(lowland, midland, (height - 0.3) / 0.2);
    } else if (height < 0.7) {
        return mix(midland, highland, (height - 0.5) / 0.2);
    } else {
        return mix(highland, peak, (height - 0.7) / 0.3);
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Transform UV by camera
    let centered_uv = (in.uv - 0.5) * 2.0;
    let world_uv = centered_uv / uniforms.scale - vec2<f32>(uniforms.offset_x, uniforms.offset_y);

    // Convert to texture coordinates (0-1 range)
    let tex_uv = world_uv * 0.5 + 0.5;

    // Check bounds
    if (tex_uv.x < 0.0 || tex_uv.x > 1.0 || tex_uv.y < 0.0 || tex_uv.y > 1.0) {
        return vec4<f32>(0.01, 0.02, 0.03, 1.0);
    }

    // Sample heightmap
    let height = textureSample(heightmap_texture, heightmap_sampler, tex_uv).r;

    // Calculate gradient for simple lighting
    let eps = 0.002;
    let h_left = textureSample(heightmap_texture, heightmap_sampler, tex_uv - vec2<f32>(eps, 0.0)).r;
    let h_right = textureSample(heightmap_texture, heightmap_sampler, tex_uv + vec2<f32>(eps, 0.0)).r;
    let h_up = textureSample(heightmap_texture, heightmap_sampler, tex_uv - vec2<f32>(0.0, eps)).r;
    let h_down = textureSample(heightmap_texture, heightmap_sampler, tex_uv + vec2<f32>(0.0, eps)).r;

    let dx = (h_right - h_left) * 10.0;
    let dy = (h_down - h_up) * 10.0;

    // Simple hillshade lighting
    let light_dir = normalize(vec3<f32>(-1.0, -1.0, 2.0));
    let normal = normalize(vec3<f32>(-dx, -dy, 1.0));
    let diffuse = max(dot(normal, light_dir), 0.0);
    let ambient = 0.3;
    let lighting = ambient + diffuse * 0.7;

    // Get terrain color based on height
    var color = terrain_color(height);

    // Apply lighting
    color *= lighting;

    // Add subtle blue tint to valleys (where rivers form)
    let valley_tint = max(0.0, 0.3 - height) * 0.5;
    color = mix(color, vec3<f32>(0.05, 0.08, 0.12), valley_tint);

    // Vignette
    let dist_from_center = length(in.uv - 0.5);
    let vignette = 1.0 - smoothstep(0.4, 1.0, dist_from_center) * 0.3;
    color *= vignette;

    return vec4<f32>(color, 1.0);
}
