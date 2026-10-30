// The radiance cache (the full path tracing plan's stage 4): the light
// arriving at the surfaces the paths reach, kept in cells of the world as
// Cyberpunk's RT Overdrive keeps it (SHaRC), so that a bounce's hit takes
// the light of every bounce after it for a lookup. A cell is a cube of the
// world, CACHE_CELL units across within CACHE_NEAR of the eye and twice as
// big with each doubling of the distance past it, for the surfaces facing
// along one axis (six to a cube). A sixteenth of the pixels trace one bounce
// further and add what arrives to the cell they leave from (cache_add);
// pt_cache.comp blends each cell's new samples into its light once a frame,
// and drops a cell no path has reached for a while. The cell's light is the
// mean light arriving, in light_of's units (a surface there sends its albedo
// times it). Define CACHE_BINDING (and include pt_frame.glsl) first.

struct CacheEntry {
    // The cell's check hash (0: an empty slot).
    uint key;
    // This frame's samples: how many, and their sum in CACHE_SCALE steps.
    uint count;
    uint sum_r;
    uint sum_g;
    uint sum_b;
    // The cell's light, blended over CACHE_FRAMES frames.
    float r;
    float g;
    float b;
    // How many frames its light holds (low byte), and how many frames since
    // a path reached it (the rest).
    uint state;
};

layout(set = 0, binding = CACHE_BINDING, scalar) buffer RadianceCache {
    CacheEntry cache[];
};

// The cache's slots (a power of two), and how many a key may move along
// from its own when its slot is taken.
const uint CACHE_SIZE = 1u << 20;
const uint CACHE_PROBES = 8u;
// The cells' size near the eye, and how near.
const float CACHE_CELL = 32.0;
const float CACHE_NEAR = 1024.0;
// A sample's light in fixed point (the sums are integers, added at once by
// many paths), and the most a sample may bring: the brightest light past
// the first bounce, a firefly otherwise.
const float CACHE_SCALE = 1024.0;
const float CACHE_CLAMP = 8.0;
// The most frames a cell's light holds, and the frames a cell no path
// reaches lasts.
const uint CACHE_FRAMES = 24u;
const uint CACHE_AGE = 300u;

#ifndef CACHE_CELLS_ONLY
uint cache_hash(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}

// The slot (x) and check hash (y, never 0) of the cell holding an
// eye-relative point facing `normal`. Cells sit on the world's grid, X and
// Z round its 65,536-unit wrap, so a cell is the same one from frame to
// frame wherever the eye goes.
uvec2 cache_key(vec3 position, vec3 normal) {
    float distance = length(position);
    int level = distance <= CACHE_NEAR ? 0 : clamp(int(ceil(log2(distance / CACHE_NEAR))), 0, 10);
    float size = CACHE_CELL * exp2(float(level));
    vec3 world = frame.eye + position;
    world.xz = mod(world.xz, 65536.0);
    ivec3 cell = ivec3(floor(world / size));
    vec3 lean = abs(normal);
    uint axis = lean.x >= lean.y && lean.x >= lean.z ? 0u : lean.y >= lean.z ? 1u : 2u;
    float along = axis == 0u ? normal.x : axis == 1u ? normal.y : normal.z;
    uint facing = axis * 2u + (along >= 0.0 ? 0u : 1u);
    uint h = cache_hash(uint(cell.x) ^ cache_hash(uint(cell.y) ^ cache_hash(
        uint(cell.z) ^ cache_hash(uint(level) * 8u + facing))));
    return uvec2(h, cache_hash(h ^ 0x9e3779b9u) | 1u);
}

// The light arriving at an eye-relative point facing `normal`, from its
// cell, or `fallback` while no path has reached the cell.
vec3 cached_light(vec3 position, vec3 normal, vec3 fallback) {
    uvec2 key = cache_key(position, normal);
    for (uint i = 0u; i < CACHE_PROBES; i++) {
        uint slot = (key.x + i) & (CACHE_SIZE - 1u);
        uint held = cache[slot].key;
        if (held == key.y) {
            return (cache[slot].state & 0xffu) > 0u
                ? vec3(cache[slot].r, cache[slot].g, cache[slot].b)
                : fallback;
        }
        if (held == 0u) {
            return fallback;
        }
    }
    return fallback;
}

// Add a sample of the light arriving at an eye-relative point facing
// `normal` to its cell, making the cell if no path has reached it.
void cache_add(vec3 position, vec3 normal, vec3 light) {
    uvec2 key = cache_key(position, normal);
    uvec3 steps = uvec3(clamp(light, vec3(0.0), vec3(CACHE_CLAMP)) * CACHE_SCALE + 0.5);
    for (uint i = 0u; i < CACHE_PROBES; i++) {
        uint slot = (key.x + i) & (CACHE_SIZE - 1u);
        uint held = atomicCompSwap(cache[slot].key, 0u, key.y);
        if (held == 0u || held == key.y) {
            atomicAdd(cache[slot].count, 1u);
            atomicAdd(cache[slot].sum_r, steps.r);
            atomicAdd(cache[slot].sum_g, steps.g);
            atomicAdd(cache[slot].sum_b, steps.b);
            return;
        }
    }
}
#endif
