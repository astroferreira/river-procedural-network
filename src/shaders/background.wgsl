// Background shader with subtle gradient and texture
// Creates a dark, atmospheric background for river visualization

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

// Simple noise function
fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);

    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));

    let u = f * f * (3.0 - 2.0 * f);
    return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}

fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var frequency = 1.0;
    var pos = p;

    for (var i = 0; i < 5; i++) {
        value += amplitude * noise(pos * frequency);
        amplitude *= 0.5;
        frequency *= 2.0;
    }

    return value;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Transform UV by camera
    let centered_uv = (in.uv - 0.5) * 2.0;
    let scaled_uv = centered_uv / uniforms.scale - vec2<f32>(uniforms.offset_x, uniforms.offset_y);

    // Base dark color
    let dark_blue = vec3<f32>(0.02, 0.05, 0.08);
    let dark_green = vec3<f32>(0.03, 0.06, 0.05);

    // Create subtle gradient from center
    let dist_from_center = length(in.uv - 0.5);
    let vignette = 1.0 - smoothstep(0.3, 0.9, dist_from_center) * 0.4;

    // Add subtle terrain-like texture
    let noise_scale = 3.0;
    let terrain_noise = fbm(scaled_uv * noise_scale);
    let terrain_variation = terrain_noise * 0.03;

    // Mix base colors based on position
    let base_color = mix(dark_blue, dark_green, terrain_noise * 0.5);

    // Add subtle "land" hints (darker areas)
    let land_noise = fbm(scaled_uv * noise_scale * 0.5 + vec2<f32>(100.0, 100.0));
    let land_mask = smoothstep(0.35, 0.65, land_noise);
    let land_color = vec3<f32>(0.04, 0.07, 0.06);

    var final_color = mix(base_color, land_color, land_mask * 0.5);

    // Apply terrain variation
    final_color += vec3<f32>(terrain_variation, terrain_variation * 0.8, terrain_variation * 0.5);

    // Subtle animated shimmer (very subtle)
    let shimmer = sin(uniforms.time * 0.5 + terrain_noise * 10.0) * 0.003;
    final_color += vec3<f32>(shimmer, shimmer * 0.5, shimmer * 0.3);

    // Apply vignette
    final_color *= vignette;

    // Ensure valid color range
    final_color = clamp(final_color, vec3<f32>(0.0), vec3<f32>(0.15));

    return vec4<f32>(final_color, 1.0);
}
