// Retail's dynamic light grid as the tracer's passes read it: 90 by 90 cells
// of 128 units, X-major, a signed RGB byte each. Declare the `grid` buffer
// and include pt_frame.glsl before this.

const int GRID_SIDE = 90;

// Retail's grid cell for a world X/Z (0044BC30, 00479508): cells of 128
// from the anchor, wrapping at 512, even cells only on coarse ground, and
// the executable's colour beyond the 90 by 90 grid.
vec3 grid_cell(vec2 world) {
    ivec2 cell = ivec2(floor(mod(world - frame.grid_anchor, 65536.0) / 128.0)) & 511;
    cell &= ~(int(frame.grid_spacing) / 128 - 1);
    if (cell.x >= GRID_SIDE || cell.y >= GRID_SIDE) {
        return frame.grid_outside;
    }
    uint rgb = grid[cell.x * GRID_SIDE + cell.y];
    return vec3(ivec3(int(rgb << 24) >> 24, int(rgb << 16) >> 24, int(rgb << 8) >> 24));
}

// Thermal vision's heat memory, behind the grid's cells: a header (the
// window's first cell as world cells of 64 units, then 1 when present),
// then 128 by 128 cells round the eye, X-major (vk_device/heat_memory.rs).
const int MEMORY_SIDE = 128;
const float MEMORY_CELL = 64.0;

// The heat the warming lights left on the ground at an eye-relative point,
// blended between the cells' middles.
float remembered_heat(vec3 position) {
    uint at = uint(GRID_SIDE * GRID_SIDE);
    if (grid.length() < int(at) + 4 || grid[at + 2u] == 0u) {
        return 0.0;
    }
    ivec2 origin = ivec2(grid[at], grid[at + 1u]);
    vec2 world = mod(frame.eye.xz + position.xz, 65536.0) / MEMORY_CELL - 0.5;
    ivec2 corner = ivec2(floor(world));
    vec2 t = world - vec2(corner);
    float heat[4];
    for (int k = 0; k < 4; k++) {
        ivec2 cell = (corner + ivec2(k & 1, k >> 1) - origin) & 1023;
        heat[k] = cell.x < MEMORY_SIDE && cell.y < MEMORY_SIDE
            ? uintBitsToFloat(grid[at + 4u + uint(cell.x * MEMORY_SIDE + cell.y)])
            : 0.0;
    }
    return mix(mix(heat[0], heat[1], t.x), mix(heat[2], heat[3], t.x), t.y);
}

// The grid at an eye-relative point, read at the terrain grid points around
// it and blended across the tile, as retail's vertex colours are; 128 is one.
vec3 grid_light(vec3 position) {
    vec2 world = frame.eye.xz + position.xz;
    float spacing = frame.grid_spacing;
    vec2 corner = floor(world / spacing) * spacing;
    vec2 t = (world - corner) / spacing;
    vec3 near_row = mix(grid_cell(corner), grid_cell(corner + vec2(spacing, 0.0)), t.x);
    vec3 far_row = mix(grid_cell(corner + vec2(0.0, spacing)),
                       grid_cell(corner + vec2(spacing)), t.x);
    return mix(near_row, far_row, t.y) / 128.0;
}
