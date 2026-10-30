// The water, shared by the tracer (pt_trace.comp) and Vulkan's raster
// (vk_mirror.comp): the game's swell and the small travelling ripples on
// it, the rain's rings and flecks, the caustics the ripples focus on the
// bed, how much of the bed shows through, the shore's wander and lapping
// line and the foam: functions of the world's position, the water's clock
// and the frame block alone. Include it after pt_frame.glsl.

uint water_hash(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}

// Value noise over the world's X/Z, 0 to 1, `period` lattice cells across
// the 65,536-unit wrap so that it meets itself there; `layer` picks the
// field.
float wrap_noise(vec2 world, int period, uint layer) {
    vec2 p = mod(world, 65536.0) * (float(period) / 65536.0);
    vec2 cell = floor(p);
    vec2 f = p - cell;
    f = f * f * (3.0 - 2.0 * f);
    ivec2 c = ivec2(cell);
    float n[4];
    for (int k = 0; k < 4; k++) {
        ivec2 corner = (c + ivec2(k & 1, k >> 1)) % period;
        n[k] = float(water_hash(uint(corner.x) + water_hash(uint(corner.y) + water_hash(layer))) >> 8)
            / 16777216.0;
    }
    return mix(mix(n[0], n[1], f.x), mix(n[2], n[3], f.x), f.y);
}

// How far from the eye the water shows the game's swell whole: what floats
// on it rides it.
const float NEAR_WATER = 1500.0;

// Small travelling ripples at a world X/Z: the slope they add to the
// water's normal, and their curvature (the height's Laplacian), which
// focuses sunlight on the bottom. A large body of water looked the same
// everywhere (2026-10-22), so slow fields over it vary them: how
// choppy they are, which of two sets leads (the second longer and turned)
// and how far their crests drift. Wave numbers and fields divide the
// 65,536-unit wrap, so the pattern meets itself across it. Twenty-four
// waves at spread angles and lengths, whose crests never line up into a
// grid (the water plan's last measures, 2026-10-25), each set as rough as
// the eight before; a wave too fine for a pixel `footprint` units across
// fades out; and the slow fields lie on a diagonal lattice, off the
// square grid their patches showed on.
void ripple_field(vec2 world, float footprint, out vec2 slope, out float curvature) {
    float time = frame.water_clock;
    vec2 diagonal = vec2(world.x + world.y, world.y - world.x);
    float chop = mix(0.3, 1.4, wrap_noise(diagonal, 96, 2u));
    float lead = smoothstep(0.3, 0.7, wrap_noise(diagonal, 48, 3u));
    vec2 drift = vec2(wrap_noise(diagonal, 64, 4u), wrap_noise(diagonal, 64, 5u)) * 400.0;
    const vec4 waves[24] = vec4[](
        vec4(641.0, 198.0, 0.0120, 0.163),
        vec4(-1133.0, 536.0, 0.0136, 0.259),
        vec4(322.0, -788.0, 0.0151, 0.204),
        vec4(552.0, 1492.0, 0.0126, 0.265),
        vec4(-961.0, -495.0, 0.0142, 0.208),
        vec4(709.0, -194.0, 0.0116, 0.199),
        vec4(-732.0, 1161.0, 0.0132, 0.260),
        vec4(-166.0, -918.0, 0.0148, 0.204),
        vec4(505.0, 384.0, 0.0122, 0.160),
        vec4(-1179.0, 108.0, 0.0138, 0.254),
        vec4(541.0, -596.0, 0.0154, 0.200),
        vec4(6.0, 1504.0, 0.0128, 0.260),
        vec4(366.0, 461.0, 0.0158, 0.172),
        vec4(-434.0, -69.0, 0.0175, 0.142),
        vec4(590.0, -389.0, 0.0147, 0.171),
        vec4(-128.0, 511.0, 0.0165, 0.140),
        vec4(-187.0, -345.0, 0.0136, 0.141),
        vec4(597.0, 207.0, 0.0154, 0.171),
        vec4(-432.0, 187.0, 0.0171, 0.140),
        vec4(309.0, -691.0, 0.0143, 0.169),
        vec4(178.0, 536.0, 0.0160, 0.170),
        vec4(-367.0, -205.0, 0.0132, 0.140),
        vec4(658.0, -156.0, 0.0149, 0.169),
        vec4(-283.0, 417.0, 0.0167, 0.138)
    );
    slope = vec2(0.0);
    curvature = 0.0;
    for (int i = 0; i < 24; i++) {
        vec2 k = waves[i].xy * (6.28318530718 / 65536.0);
        float size = length(k);
        // Half a wave a pixel is the finest a pixel can show.
        float seen = 1.0 - smoothstep(0.25, 0.5, size * footprint / 6.28318530718);
        float share = (i < 12 ? 1.0 - lead : lead) * chop * waves[i].z * seen;
        float phase = dot(k, world + drift) + waves[i].w * time;
        // The height share / |k| sin(phase): its slope and its Laplacian.
        slope += share * k / size * cos(phase);
        curvature -= share * size * sin(phase);
    }
}

// The water's normal at an eye-relative point: beyond NEAR_WATER only part
// of the game's swell shows, by a slow field (its heights stay the game's,
// which what floats rides), and the ripples ride on it.
vec3 rippled(vec3 normal, vec3 position) {
    vec2 world = frame.eye.xz + position.xz;
    float far = smoothstep(NEAR_WATER, 2.0 * NEAR_WATER, length(position.xz));
    float swell = mix(1.0, mix(0.2, 1.0, wrap_noise(world, 48, 1u)), far);
    vec3 up = vec3(0.0, normal.y < 0.0 ? -1.0 : 1.0, 0.0);
    normal = normalize(mix(up, normal, swell));
    vec2 slope;
    float curvature;
    ripple_field(world, length(position) / frame.focal.x, slope, curvature);
    // Seen from below, the same surface's slopes (as the caustics and the
    // shafts under it bend by), its normal turned down.
    return normalize(normal - up.y * vec3(slope.x, 0.0, slope.y));
}

// How many clarities into the water the bed fades to 1/e (water_clear).
const float WATER_SHOULDER = 1.5;

// How strongly the ripples focus: 2.4 times the bend water's index gives
// (1 - 1/1.33), since the ripples are gentle for their depth and the
// pattern would hardly show; and the most light a point gathers. At four
// times, capped at 3, a sixth of a sunlit bed sat at the cap, the flat
// golden cells that read as tiles (the water plan's step 2 constants;
// chosen 2026-10-23: the river as a mirror, the sandbar smooth).
const float CAUSTIC_FOCUS = 0.6;

const float CAUSTIC_CAP = 2.2;

// How deep the caustics have blurred away to nothing.
const float CAUSTIC_DEPTH = 800.0;

// How much the ripples focus the sunlight reaching an eye-relative point
// `depth` below the water (caustics), the ripples finer than `footprint`
// units left out: the beam bends into the water where it entered,
// converging under the ripples' crests and spreading under their troughs
// over the depth, which blurs the pattern away deeper down.
float caustic_seen(vec3 bottom, float depth, float footprint) {
    vec3 beam = refract(-frame.sun_direction, vec3(0.0, 1.0, 0.0), 1.0 / 1.333);
    vec2 entry = frame.eye.xz + bottom.xz - beam.xz * (depth / max(-beam.y, 0.2));
    vec2 slope;
    float curvature;
    ripple_field(entry, footprint, slope, curvature);
    float focus = 1.0 / max(1.0 + depth * CAUSTIC_FOCUS * curvature, 1.0 / CAUSTIC_CAP);
    return mix(1.0, focus, 1.0 - smoothstep(250.0, CAUSTIC_DEPTH, depth));
}

// The caustics on a point of the bottom, as a pixel there sees them.
float caustic(vec3 bottom, float depth) {
    return caustic_seen(bottom, depth, length(bottom) / frame.focal.x);
}

// The same, none worked out where they have blurred away.
float deep_caustic(vec3 bottom, float depth) {
    return depth < CAUSTIC_DEPTH ? caustic(bottom, depth) : 1.0;
}

// Value noise in three dimensions, 0 to 1, a lattice cell a unit.
float value_noise(vec3 p) {
    vec3 cell = floor(p);
    vec3 f = p - cell;
    f = f * f * (3.0 - 2.0 * f);
    uvec3 c = uvec3(ivec3(cell) + ivec3(1 << 20));
    float n[8];
    for (int k = 0; k < 8; k++) {
        uvec3 corner = c + uvec3(k & 1, (k >> 1) & 1, (k >> 2) & 1);
        n[k] = float(water_hash(corner.x + water_hash(corner.y + water_hash(corner.z))) >> 8) / 16777216.0;
    }
    return mix(mix(mix(n[0], n[1], f.x), mix(n[2], n[3], f.x), f.y),
               mix(mix(n[4], n[5], f.x), mix(n[6], n[7], f.x), f.y), f.z);
}

// The rain's own rings on the water: in cells fixed on the surface, one
// ring at a time spreading from a hashed spot, a new spot each time, in as
// many cells as the rain is dense.
const float RAIN_RING_CELL = 48.0;

vec3 rained(vec3 normal, vec3 position) {
    if (frame.precip_kind < 1.5 || frame.precip_density <= 0.0) {
        return normal;
    }
    float time = frame.water_clock / 50.0;
    vec2 world = (frame.eye.xz + position.xz) / RAIN_RING_CELL;
    vec2 base = floor(world);
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            ivec2 cell = ivec2(base) + ivec2(i, j);
            uint h = water_hash(uint(cell.x) * 73856093u ^ uint(cell.y) * 19349663u);
            if (float(h & 0xffu) / 255.0 > frame.precip_density) {
                continue;
            }
            float rate = 0.8 + 0.8 * float((h >> 8) & 0xffu) / 255.0;
            float age = time * rate + float((h >> 16) & 0xffu) / 255.0;
            float life = fract(age);
            uint spot_hash = water_hash(h ^ uint(floor(age)) * 2654435761u);
            vec2 spot = vec2(cell) + 0.2
                + 0.6 * vec2(float(spot_hash & 0xffu), float((spot_hash >> 8) & 0xffu)) / 255.0;
            vec2 offset = (world - spot) * RAIN_RING_CELL;
            float distance = length(offset);
            float x = (distance - 30.0 * life) / 3.0;
            float strength = (1.0 - life) * (1.0 - life) * 0.35;
            normal.xz += offset / max(distance, 1.0) * exp(-x * x) * sin(x * 3.14159265) * strength;
        }
    }
    return normal;
}

// The drops striking the water (2026-10-26: splashes when it rains on the
// water too): at each of `rained`'s rings, as it
// starts, a bright fleck where the drop hit, gone in a third of the ring's
// life; 0 to 1 at `position`, fading out by 2,500 units.
float rain_flecks(vec3 position) {
    if (frame.precip_kind < 1.5 || frame.precip_kind > 2.5 || frame.precip_density <= 0.0) {
        return 0.0;
    }
    float near = 1.0 - smoothstep(1200.0, 2500.0, length(position));
    if (near <= 0.0) {
        return 0.0;
    }
    float time = frame.water_clock / 50.0;
    vec2 world = (frame.eye.xz + position.xz) / RAIN_RING_CELL;
    vec2 base = floor(world);
    float fleck = 0.0;
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            ivec2 cell = ivec2(base) + ivec2(i, j);
            uint h = water_hash(uint(cell.x) * 73856093u ^ uint(cell.y) * 19349663u);
            if (float(h & 0xffu) / 255.0 > frame.precip_density) {
                continue;
            }
            float rate = 0.8 + 0.8 * float((h >> 8) & 0xffu) / 255.0;
            float age = time * rate + float((h >> 16) & 0xffu) / 255.0;
            float life = fract(age) / 0.3;
            if (life >= 1.0) {
                continue;
            }
            uint spot_hash = water_hash(h ^ uint(floor(age)) * 2654435761u);
            vec2 spot = vec2(cell) + 0.2
                + 0.6 * vec2(float(spot_hash & 0xffu), float((spot_hash >> 8) & 0xffu)) / 255.0;
            float distance = length(world - spot) * RAIN_RING_CELL;
            float crown = exp(-pow((distance - 4.0 * life) / 1.2, 2.0));
            fleck = max(fleck, (exp(-pow(distance / 1.8, 2.0)) + 0.6 * crown) * (1.0 - life));
        }
    }
    return fleck * near;
}

// How much of the water at `position` foam covers, from `foam` (0 to 1):
// a thin, soft froth, strongest where the water was hit and gone as it
// fades, broken into slow patches, never opaque. It has no cells: the
// bubbles' walls of 2026-10-23 read as a spider web on the water.
float foam_cover(vec3 position, float foam) {
    if (foam <= 0.01) {
        return 0.0;
    }
    vec2 world = frame.eye.xz + position.xz;
    float time = frame.water_clock * 0.03;
    float froth = 0.6 * value_noise(vec3(world / 9.0, time))
        + 0.4 * value_noise(vec3(world / 3.5 + 17.0, time * 2.0));
    float patches = value_noise(vec3(world / 70.0, time));
    float amount = foam * mix(0.5, 1.0, patches);
    return 0.6 * smoothstep(1.0 - amount, 1.35 - amount, froth);
}

// How much of the liquid's colour its foam takes.
const float FOAM_TINT = 0.25;

// How much of what lies `t` along a ray into the water shows through it,
// for the world's clarity: clear over the shallows, then closing over the
// deep, gone by about three clarities (2026-10-25: the water went
// opaque quite early; a plain exp(-t / clarity) had already hidden two
// fifths of the bed a clarity's half in).
float water_clear(float t) {
    float x = t / (WATER_SHOULDER * frame.water_clarity);
    return exp(-x * x);
}

// How far the shore's edge wanders at world X/Z, in units of depth: a slow
// noise, the same for the water's fade and the damp band, so they meet.
float shore_wander(vec2 world) {
    return (wrap_noise(world, 512, 11u) - 0.5) * 2.0 * frame.shore_wobble;
}

// The thin foam line at the shore, at an eye-relative point of the surface
// whose shore depth (shore_depth's, wander and all) is `shore`: it creeps
// slowly up and back as small waves lap, broken along its length.
float lap_line(vec3 position, float shore) {
    if (frame.shore_lap <= 0.0) {
        return 0.0;
    }
    vec2 world = frame.eye.xz + position.xz;
    float time = frame.water_clock / 50.0;
    float lap = 1.5 + 2.0 * (0.5 + 0.5 * sin(time * 0.7
                                             + 6.28318530718 * wrap_noise(world, 256, 13u)));
    float line = 1.0 - smoothstep(0.0, 2.0, abs(shore - lap));
    return frame.shore_lap * line * smoothstep(0.3, 0.6, wrap_noise(world, 2048, 14u));
}

// The headlamps under the water light the water itself
// (2026-10-24, up to a point): along the refracted ray's way
// down from `start` (`reach` long), at eight points, the beams of the lamps
// under the surface scattered back in the water's own colour, absorbed by
// its clarity on their way from the lamp and on the way back up. Lamps
// above the water light its surface instead (water_color); with the eye
// under it, the mist's gathering does this along the view.
vec3 lamps_in_water(vec3 start, vec3 bent, float reach) {
    if (frame.lamp_count < 0.5) {
        return vec3(0.0);
    }
    float clarity = max(frame.water_clarity, 1.0);
    float span = min(reach, 3.0 * clarity);
    float step = span / 8.0;
    vec3 sum = vec3(0.0);
    for (int k = 0; k < 8; k++) {
        float s = (float(k) + 0.5) * step;
        vec3 point = start + bent * s;
        vec3 here = vec3(0.0);
        for (int i = 0; i < int(frame.lamp_count); i++) {
            if (frame.eye.y + frame.lamps[i].position.y >= frame.water_level) {
                continue;
            }
            vec3 toward;
            float distance;
            vec3 beam = glowing_beam(frame.lamps[i], point, toward, distance);
            here += beam * exp(-distance / clarity);
        }
        sum += here * (exp(-s / clarity) * step / clarity);
    }
    return sum * frame.lamp_water_hue * frame.lamp_water_glow;
}
