# Procedural River Network Generator

A beautiful procedural river network visualization inspired by watershed maps, written in Rust with GPU-accelerated rendering.

![River Network Example](https://user-images.githubusercontent.com/placeholder/river-network.png)

## Features

- **Procedural Terrain Generation**: Multi-octave noise-based heightmap generation with realistic drainage patterns
- **Hydrology Simulation**: D8 flow direction algorithm with flow accumulation calculation
- **Watershed Delineation**: Automatic identification and coloring of distinct watersheds/drainage basins
- **Stream Order Calculation**: Strahler stream ordering for realistic river hierarchy
- **GPU-Accelerated Rendering**: wgpu-based rendering with custom WGSL shaders
- **Beautiful Visualization**: Color-coded watersheds with glow effects and smooth animations
- **Interactive Controls**: Pan, zoom, and regenerate terrain in real-time

## Controls

| Key/Action | Description |
|------------|-------------|
| Mouse Drag | Pan the view |
| Mouse Wheel | Zoom in/out |
| Space | Generate new random terrain |
| R | Reset view to default |
| +/- | Adjust river detail (flow threshold) |
| S/A | Increase/decrease terrain size |
| Escape | Quit |

## Building

### Prerequisites

- Rust 1.70 or later
- A GPU with Vulkan, Metal, or DX12 support

### Build & Run

```bash
# Clone the repository
git clone https://github.com/your-username/river-procedural-network.git
cd river-procedural-network

# Build in release mode (recommended for performance)
cargo build --release

# Run
cargo run --release
```

## Technical Details

### Terrain Generation

The terrain is generated using multiple layers of Fractal Brownian Motion (fBm) noise:
- Base terrain layer with 8 octaves
- Ridge mountains using absolute value transformation
- Valley carving using squared noise values
- Edge falloff for island-like appearance

### Hydrology Simulation

1. **D8 Flow Direction**: Each cell flows to its steepest downhill neighbor
2. **Flow Accumulation**: Topological sort-based upstream area calculation
3. **Watershed Delineation**: Tracing from pour points to identify distinct basins
4. **Stream Order**: Strahler ordering system for river hierarchy

### Rendering

- Custom WGSL shaders for river visualization
- Per-watershed color palette for distinct basin identification
- Flow-based line thickness for visual hierarchy
- Subtle glow effects for major rivers
- Dark atmospheric background with terrain hints

## Architecture

```
src/
├── main.rs          # Application entry point and event loop
├── terrain.rs       # Procedural terrain generation
├── hydrology.rs     # Flow direction, accumulation, and watershed analysis
├── renderer.rs      # wgpu rendering pipeline
├── lib.rs           # Library exports
└── shaders/
    ├── river.wgsl      # River rendering shader
    └── background.wgsl # Background rendering shader
```

## Dependencies

- `wgpu` - GPU rendering
- `winit` - Windowing
- `noise` - Procedural noise generation
- `bytemuck` - Safe transmutation
- `rayon` - Parallel processing (for future optimizations)
- `rand` - Random number generation

## License

MIT License - feel free to use this for any purpose.

## Acknowledgments

Inspired by beautiful watershed visualization maps like those created by Grasshopper Geography.
