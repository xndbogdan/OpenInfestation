// The world's snow or rain (the sky plan's weather), drawn by the composite
// over everything at the display's size. Include it after pt_frame.glsl.
//
// Flakes and drops hang in cells fixed in the world, one at a hashed spot
// in the middle of each cell the density keeps, the whole field falling
// and drifting with the game's clock (never its random numbers). Each view
// ray walks the cells it crosses in four shells about the eye, whose cells
// grow with their distance: the near fall is coarse and close, the far fall
// fine, and all of it fills the view. A cell belongs to a shell by its own
// middle's distance, the same for every ray, so no flake is ever cut; it
// fades in and out at the shell's edges as the eye moves. Nothing shows
// behind nearer geometry, and nothing under the water: the fall stops at
// its plane (2026-10-28: it was raining under the water), and
// with the eye under it none shows at all.

// The nearest shell's distance, in world units; each next shell is twice
// as far, its cells twice as wide.
const float PRECIP_NEAR = 200.0;
// A shell's cells against its distance, and its depth in cells.
const float PRECIP_CELL = 0.1;
const float PRECIP_DEPTH = 2.0;

// How far under the water's plane the fall still shows, as it strikes it.
const float PRECIP_UNDER = 2.0;

// Whether an eye-relative point lies under the water (no water: never).
bool under_water(vec3 point, float below) {
    return frame.eye.y + point.y < frame.water_level - below;
}

uint precip_hash(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}

// The precipitation along `direction` in front of `scene` (the distance to
// what the ray meets, along it): its light, already lit, and how much of
// what lies behind it it hides.
vec4 precipitation(vec3 direction, float scene) {
    if (frame.precip_kind < 0.5 || frame.precip_density <= 0.0 || frame.thermal > 0.5
        || under_water(vec3(0.0), 0.0)) {
        return vec4(0.0);
    }
    bool rain = frame.precip_kind > 1.5 && frame.precip_kind < 2.5;
    bool embers = frame.precip_kind > 3.5;
    float time = frame.water_clock / 50.0;
    vec3 velocity = vec3(frame.precip_wind.x, -frame.precip_fall, frame.precip_wind.y);
    vec3 motion = velocity * time;
    vec3 fall = normalize(velocity + vec3(0.0, -1.0e-3, 0.0));
    // Lit by the sky all round and a little by the sun, in the fall's own
    // colour; never brighter than white.
    vec3 light = min((frame.ambient * 2.0 + frame.sun_color * 0.5) * frame.precip_color, vec3(1.0));
    // The walk runs in the falling field's frame, from the eye.
    vec3 origin = frame.eye - motion;
    vec3 step_sign = sign(direction);
    vec3 inverse = 1.0 / max(abs(direction), vec3(1.0e-6));
    float cover = 0.0;
    vec3 glow = vec3(0.0);
    for (int shell = 0; shell < 4; shell++) {
        float near = PRECIP_NEAR * exp2(float(shell));
        float cell = near * PRECIP_CELL;
        float far = near + PRECIP_DEPTH * cell;
        // From a cell before the shell to a cell past it: the cells whose
        // middles lie in it all meet the ray in that span.
        float t = max(near - cell, 0.0);
        float end = min(far + cell, scene);
        vec3 start = origin + direction * t;
        ivec3 index = ivec3(floor(start / cell));
        // How far along the ray each axis's next cell wall lies.
        vec3 next = ((vec3(index) + max(step_sign, 0.0)) * cell - start) * step_sign * inverse + t;
        vec3 delta = cell * inverse;
        // Strength falls with the shell, as the air between thickens; far
        // snow faintest, so the few near flakes carry it (2026-10-26:
        // the snow was a bit too intrusive).
        float strength = rain ? 0.35 * (1.0 - 0.18 * float(shell))
                              : 0.55 * (1.0 - 0.27 * float(shell));
        for (int walk = 0; walk < 8 && t < end; walk++) {
            vec3 middle = (vec3(index) + 0.5) * cell + motion - frame.eye;
            float away = length(middle);
            float inside = smoothstep(near - 0.5 * cell, near, away)
                * (1.0 - smoothstep(far - 0.5 * cell, far, away));
            uint h = precip_hash(uint(index.x) * 73856093u ^ uint(index.y) * 19349663u
                                 ^ uint(index.z) * 83492791u ^ uint(shell) * 2654435761u);
            if (inside > 0.0 && float(h & 0xffffu) / 65536.0 < frame.precip_density) {
                vec3 spot = vec3(float((h >> 16) & 0xffu), float(h >> 24),
                                 float(precip_hash(h) & 0xffu)) / 255.0;
                vec3 offset = (vec3(index) + 0.3 + 0.4 * spot) * cell + motion - frame.eye;
                // A flake sways on its own beat as it falls, within its cell.
                float beat = float(precip_hash(h ^ 0x51ab1eu) & 0xffffu) / 65536.0;
                if (!rain && !embers) {
                    float sway = 0.12 * cell;
                    offset.xz += sway * vec2(sin(time * (0.5 + 0.6 * beat) + beat * 40.0),
                                             cos(time * (0.4 + 0.5 * beat) + beat * 23.0));
                }
                float along = dot(offset, direction);
                // Where the view passes the drop: a streak ends at the
                // water's plane.
                if (along > 0.0 && along < scene
                    && !under_water(direction * along, PRECIP_UNDER)) {
                    vec3 across = offset - direction * along;
                    float r;
                    if (rain) {
                        // A drop is a streak along its fall.
                        float lengthwise = dot(across, fall);
                        r = length(vec2(length(across - fall * lengthwise) / frame.precip_size,
                                        lengthwise / max(frame.precip_length, frame.precip_size)));
                    } else {
                        // Flakes of many sizes, most of them small.
                        float size = frame.precip_size * (0.5 + beat * beat * 1.0);
                        r = length(across) / size;
                    }
                    float seen = exp((rain ? -2.0 : -1.6) * r * r) * strength * inside;
                    if (embers) {
                        // A spark glows in its own colour, flickering on its
                        // own beat.
                        float beat = float(precip_hash(h ^ 0x9e3779b9u) & 0xffffu) / 65536.0;
                        float flicker = 0.55 + 0.45 * sin(time * (6.0 + 8.0 * beat) + beat * 40.0);
                        glow = max(glow, frame.precip_color * (1.6 * flicker * seen));
                    }
                    cover = max(cover, seen);
                }
            }
            // Into the next cell, across the nearest wall.
            if (next.x < next.y && next.x < next.z) {
                t = next.x;
                next.x += delta.x;
                index.x += int(step_sign.x);
            } else if (next.y < next.z) {
                t = next.y;
                next.y += delta.y;
                index.y += int(step_sign.y);
            } else {
                t = next.z;
                next.z += delta.z;
                index.z += int(step_sign.z);
            }
        }
    }
    if (embers) {
        // Sparks add light; they hide little.
        return vec4(glow, 0.2 * cover);
    }
    return vec4(light * cover, cover);
}

// The rain's drops striking the ground near the eye: in cells fixed on the
// ground, each in turn a fleck and a small ring spreading from a hashed
// spot for a tenth of a second, in as many cells as the rain is dense, on
// faces turned up (`up`: their normal's height), never on the water or the
// bed under it (the water's own rings and flecks are its), nor with the eye
// under the water. `point` is eye-relative. Returns the splash's share of
// the rain's light.
const float DROP_CELL = 40.0;
const float DROP_SECONDS = 0.12;
float drop_splashes(vec3 point, float up, float distance) {
    if (frame.precip_kind < 1.5 || frame.precip_kind > 2.5 || frame.precip_density <= 0.0
        || frame.thermal > 0.5 || under_water(vec3(0.0), 0.0) || under_water(point, -1.0)) {
        return 0.0;
    }
    float near = 1.0 - smoothstep(500.0, 1400.0, distance);
    if (near <= 0.0 || up < 0.6) {
        return 0.0;
    }
    float time = frame.water_clock / 50.0;
    vec2 world = (frame.eye.xz + point.xz) / DROP_CELL;
    vec2 base = floor(world);
    float splash = 0.0;
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            ivec2 cell = ivec2(base) + ivec2(i, j);
            uint h = precip_hash(uint(cell.x) * 73856093u ^ uint(cell.y) * 19349663u ^ 0x5157u);
            if (float(h & 0xffu) / 255.0 > 0.8 * frame.precip_density) {
                continue;
            }
            float rate = 1.0 + 1.5 * float((h >> 8) & 0xffu) / 255.0;
            float age = time * rate + float((h >> 16) & 0xffu) / 255.0;
            float seconds = fract(age) / rate;
            if (seconds > DROP_SECONDS) {
                continue;
            }
            uint spot_hash = precip_hash(h ^ uint(floor(age)) * 2654435761u);
            vec2 spot = vec2(cell) + 0.15
                + 0.7 * vec2(float(spot_hash & 0xffu), float((spot_hash >> 8) & 0xffu)) / 255.0;
            float d = length(world - spot) * DROP_CELL;
            float life = seconds / DROP_SECONDS;
            float radius = 1.0 + 4.5 * life;
            float ring = exp(-pow((d - radius) / 0.7, 2.0));
            float fleck = exp(-pow(d / 1.1, 2.0)) * (1.0 - life);
            splash = max(splash, (0.6 * ring + fleck) * (1.0 - life));
        }
    }
    return splash * near * smoothstep(0.6, 0.85, up);
}

