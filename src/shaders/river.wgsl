// River rendering shader with beautiful glow and color effects
// Inspired by watershed visualization maps

struct Uniforms {
    resolution: vec2<f32>,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    time: f32,
    max_flow: f32,
    num_watersheds: f32,
}

struct PaletteColor {
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var<storage, read> palette: array<PaletteColor>;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) flow: f32,
    @location(2) stream_order: f32,
    @location(3) watershed_id: f32,
    @location(4) progress: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) flow: f32,
    @location(1) stream_order: f32,
    @location(2) watershed_id: f32,
    @location(3) progress: f32,
    @location(4) world_pos: vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Apply scale and offset for pan/zoom
    let scaled_pos = in.position * uniforms.scale + vec2<f32>(uniforms.offset_x, uniforms.offset_y);

    // Correct aspect ratio
    let aspect = uniforms.resolution.x / uniforms.resolution.y;
    var final_pos = scaled_pos;
    if aspect > 1.0 {
        final_pos.x /= aspect;
    } else {
        final_pos.y *= aspect;
    }

    out.clip_position = vec4<f32>(final_pos, 0.0, 1.0);
    out.flow = in.flow;
    out.stream_order = in.stream_order;
    out.watershed_id = in.watershed_id;
    out.progress = in.progress;
    out.world_pos = in.position;

    return out;
}

// Helper function for smooth color transitions
fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

// HSL to RGB conversion for color generation
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> vec3<f32> {
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let x = c * (1.0 - abs(((h / 60.0) % 2.0) - 1.0));
    let m = l - c / 2.0;

    var rgb: vec3<f32>;
    let h_sector = h / 60.0;

    if h_sector < 1.0 {
        rgb = vec3<f32>(c, x, 0.0);
    } else if h_sector < 2.0 {
        rgb = vec3<f32>(x, c, 0.0);
    } else if h_sector < 3.0 {
        rgb = vec3<f32>(0.0, c, x);
    } else if h_sector < 4.0 {
        rgb = vec3<f32>(0.0, x, c);
    } else if h_sector < 5.0 {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }

    return rgb + vec3<f32>(m, m, m);
}

// Generate watershed color using golden ratio for good distribution
fn get_watershed_color(id: f32) -> vec3<f32> {
    // Use palette if available
    let palette_index = u32(id) % 256u;
    let palette_color = palette[palette_index].color.rgb;

    // Add subtle variation based on exact ID
    let variation = fract(id * 0.618033988749895); // Golden ratio
    let hue_shift = variation * 30.0 - 15.0; // +/- 15 degrees

    // Convert to HSL, shift, convert back (simplified approximation)
    let luminance = dot(palette_color, vec3<f32>(0.299, 0.587, 0.114));
    let saturation = length(palette_color - vec3<f32>(luminance)) * 1.5;

    // Apply subtle variation
    let varied_color = palette_color + vec3<f32>(
        sin(hue_shift * 0.1) * 0.1,
        cos(hue_shift * 0.15) * 0.05,
        sin(hue_shift * 0.12) * 0.08
    );

    return clamp(varied_color, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Get base watershed color
    let base_color = get_watershed_color(in.watershed_id);

    // Calculate intensity based on flow (normalized)
    let flow_normalized = clamp(in.flow / max(uniforms.max_flow, 1.0), 0.0, 1.0);

    // Stream order affects brightness - higher orders are brighter
    let order_factor = 0.3 + in.stream_order * 0.1;

    // Create glow effect based on flow
    let glow_intensity = smootherstep(0.0, 1.0, flow_normalized) * 0.6;

    // Brighter core for high-flow rivers
    let core_brightness = 0.4 + flow_normalized * 0.6;

    // Final color calculation
    var final_color = base_color * core_brightness;

    // Add subtle glow (brighter towards center of river)
    let glow_color = base_color * 1.3;
    final_color = mix(final_color, glow_color, glow_intensity);

    // Slight color variation along the river for visual interest
    let length_variation = sin(in.progress * 3.14159 * 2.0 + uniforms.time * 0.5) * 0.03;
    final_color += vec3<f32>(length_variation, length_variation * 0.5, -length_variation);

    // Apply order factor (higher order streams are more prominent)
    final_color *= order_factor;

    // Small tributaries are slightly darker and more transparent
    let alpha = 0.6 + flow_normalized * 0.4;

    // Subtle pulsing animation for major rivers
    let pulse = 1.0 + sin(uniforms.time * 2.0) * 0.02 * flow_normalized;
    final_color *= pulse;

    // Ensure colors stay in valid range with nice saturation
    final_color = clamp(final_color, vec3<f32>(0.0), vec3<f32>(1.0));

    return vec4<f32>(final_color, alpha);
}
