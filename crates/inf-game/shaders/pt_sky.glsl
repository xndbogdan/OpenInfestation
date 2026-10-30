// The sky the renderer draws where a world's recipe asks for it (the sky
// plan, docs/rt-plans/sky-per-world.md): a gradient from the zenith's
// colour to the horizon's, the sun where the world's light comes from,
// stars where it is dark, and a layer of volumetric clouds. Include it
// after pt_frame.glsl; define SKY_NOISE_BINDING (two bindings: the shape
// noise, then the detail noise) for the clouds' march.

// The cloud shadow map's span, in world units, centred on the eye.
const float SKY_SHADOW_SPAN = 32768.0;
// The view's clouds keep how far along each ray they lie, in units of this
// many world units (half floats hold up to 65,504); clear sky keeps the
// most they hold.
const float CLOUD_DISTANCE_UNIT = 16.0;
const float CLOUD_DISTANCE_NONE = 65504.0;
// The world wraps every 65,536 units; the clouds' noise tiles with it.
const float SKY_WRAP = 65536.0;

// The share of the view's clouds (`clouds`: their light, and what shows
// through them) that lies in front of a surface `along` the view, by where
// they lie (`placed`, pt_sky_clouds.comp's: in CLOUD_DISTANCE_UNIT the
// distance of the light beyond the near band, what the near band lets
// through and its share of the light): their light in rgb and what of the
// surface still shows in alpha. The near band, the clouds between the eye
// and the craft it follows (frame.cloud_craft), lies in front of all
// beyond it, and of a nearer surface by its distance; the rest in front of
// a surface well beyond their distance, or `beyond` of it at least (a
// world faded into the fog, which takes them as the sky does).
vec4 clouds_before(vec4 clouds, vec4 placed, float along, float beyond) {
    float craft = frame.cloud_craft;
    float near_through = clamp(placed.g, 1.0e-4, 1.0);
    vec3 near_light = clouds.rgb * placed.b;
    if (along < craft) {
        float part = along / craft;
        float through = pow(near_through, part);
        float lit = near_through < 0.999 ? (1.0 - through) / (1.0 - near_through) : part;
        return vec4(near_light * lit, through);
    }
    float away = placed.r * CLOUD_DISTANCE_UNIT;
    float front = smoothstep(max(0.75 * away, craft), max(1.25 * away, 1.1 * craft) + 1.0, along);
    front = max(front, beyond);
    float far_through = clamp(clouds.a / near_through, 0.0, 1.0);
    return vec4(near_light + (clouds.rgb - near_light) * front,
                near_through * mix(1.0, far_through, front));
}

uint sky_hash(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}

// The world's light at its brightest channel one.
vec3 sun_hue() {
    return frame.sun_color / max(1.0, max(frame.sun_color.r,
                                          max(frame.sun_color.g, frame.sun_color.b)));
}

// The world hanging in the sky along `direction` (the homeworld), and how
// much of what lies behind it it hides: a sphere far off, lit by the sun
// on one side, its surface cut into vast panels by seams along its
// latitudes and longitudes, its night side pricked with warm lights, a
// thin haze at its rim.
vec4 sky_planet(vec3 direction) {
    if (frame.planet_cos <= 0.0) {
        return vec4(0.0);
    }
    vec3 middle = frame.planet_direction;
    float along = dot(direction, middle);
    float sine = sqrt(max(1.0 - frame.planet_cos * frame.planet_cos, 1.0e-8));
    vec3 across = (direction - middle * along) / sine;
    float r = length(across);
    float edge = 2.0e-3;
    float cover = (1.0 - smoothstep(1.0 - edge, 1.0 + edge, r)) * step(0.0, along);
    if (cover <= 0.0) {
        return vec4(0.0);
    }
    float depth = sqrt(max(1.0 - r * r, 0.0));
    vec3 normal = normalize(across - middle * depth);
    vec3 east = normalize(cross(vec3(0.0, 1.0, 0.0), middle) + vec3(1.0e-4, 0.0, 0.0));
    vec3 north = cross(middle, east);
    vec3 local = vec3(dot(across, east), dot(across, north), depth);
    float latitude = asin(clamp(local.y, -1.0, 1.0));
    float longitude = atan(local.x, local.z);
    vec2 grid = vec2(longitude, latitude) * (9.0 / 3.14159265);
    vec2 seam = abs(fract(grid) - 0.5);
    float panel = 1.0 - 0.45 * (1.0 - smoothstep(0.0, 0.035, 0.5 - max(seam.x, seam.y)));
    float lit = max(dot(normal, frame.sun_direction), 0.0);
    vec3 color = frame.planet_color * (0.02 + lit) * panel * sun_hue();
    vec2 cell = floor(grid * 6.0);
    uint seed = sky_hash(uint(int(cell.x) + 4096) ^ sky_hash(uint(int(cell.y) + 4096)));
    if ((seed & 0xffu) < 22u) {
        vec2 spot = fract(grid * 6.0) - 0.5;
        float light = exp(-dot(spot, spot) * 60.0) * (1.0 - smoothstep(0.0, 0.25, lit));
        color += vec3(1.0, 0.62, 0.3) * frame.planet_lights * light;
    }
    color += sun_hue() * frame.planet_color * 0.35 * pow(r, 6.0) * smoothstep(-0.2, 0.4, dot(across, frame.sun_direction));
    return vec4(color, cover);
}

// The sky along `direction` without its clouds: the zenith's colour
// overhead giving way to the horizon's, where the haze meets it; a glow
// round the sun and its disc (`disc`, its share: none in the cube, whose
// rays the sun's own rays already light), both where the world's light
// comes from and in its colour; a warm glow low toward the sun for the
// sunset worlds; and hashed stars where it is dark.
vec3 sky_gradient(vec3 direction, float disc) {
    float up = clamp(direction.y, 0.0, 1.0);
    vec3 color = mix(frame.sky_horizon, frame.sky_zenith, 1.0 - pow(1.0 - up, frame.sky_curve));
    vec3 sun = sun_hue();
    float facing = dot(direction, frame.sun_direction);
    float g = 0.76;
    float glow = (1.0 - g * g) / pow(max(1.0 + g * g - 2.0 * g * facing, 1.0e-4), 1.5);
    color += sun * frame.sky_glow * 0.02 * glow;
    if (frame.sky_horizon_glow > 0.0) {
        vec2 across = direction.xz;
        vec2 toward_sun = frame.sun_direction.xz;
        float toward = max(dot(across, toward_sun), 0.0)
            / max(length(across) * length(toward_sun), 1.0e-4);
        color += sun * vec3(1.0, 0.72, 0.45) * frame.sky_horizon_glow * toward * toward
            * exp(-abs(direction.y) * 6.0);
    }
    float rim = (1.0 - frame.sun_cone) * 0.15 + 1.0e-6;
    color = mix(color, max(sun, color),
                disc * smoothstep(frame.sun_cone - rim, frame.sun_cone + rim, facing));
    vec4 planet = sky_planet(direction);
    color = mix(color, planet.rgb, planet.a);
    if (frame.sky_stars > 0.0 && direction.y > 0.0 && planet.a < 0.5) {
        uvec3 cell = uvec3(ivec3(floor(direction * 600.0)) + 1024);
        uint seed = sky_hash(cell.x ^ sky_hash(cell.y ^ sky_hash(cell.z)));
        float chance = float(seed & 0xffffu) / 65536.0;
        float dark = 1.0 - max(color.r, max(color.g, color.b));
        if (chance < 0.004 * frame.sky_stars) {
            color += vec3(0.6 + 0.4 * float(seed >> 24) / 255.0) * dark * up;
        }
    }
    return color;
}

// The lightning at the game's clock (50 ticks a second): the direction
// across the ground toward the flash (xz) and its brightness (w, 0 to 1).
// Time runs in slots of the mean spacing; each slot's hash says whether it
// flashes (three in four), when, and where. A flash is a bright stroke and
// a weaker return, gone in under half a second; never its random numbers.
vec4 lightning_flash() {
    if (frame.lightning_rate <= 0.0) {
        return vec4(0.0);
    }
    float seconds = frame.water_clock / 50.0;
    float spacing = 1.0 / frame.lightning_rate;
    float slot = floor(seconds / spacing);
    uint h = sky_hash(uint(slot) * 747796405u + 2891336453u);
    if ((h & 3u) == 0u) {
        return vec4(0.0);
    }
    float since = seconds - (slot + 0.1 + 0.6 * float((h >> 2) & 0xffffu) / 65536.0) * spacing;
    if (since < 0.0 || since > 0.45) {
        return vec4(0.0);
    }
    float stroke = exp(-since * 25.0) + 0.6 * step(0.12, since) * exp(-(since - 0.12) * 16.0);
    float turn = float(h >> 18) / 16384.0 * 6.28318530718;
    return vec4(cos(turn), 0.0, sin(turn), min(stroke, 1.0));
}

// The direction a flash's light comes from: toward it across the ground,
// from high in the clouds.
vec3 flash_toward(vec4 flash) {
    return normalize(vec3(flash.x, 0.8, flash.z));
}

// A near strike's bolt along `direction` (one flash in four, as the
// thunder's near class): a jagged channel from the cloud base down to the
// horizon toward the flash, a few forks, white-hot in the core and a halo
// in the lightning's colour, gone with the stroke. Additive.
vec3 lightning_bolt(vec3 direction) {
    if (frame.lightning_rate <= 0.0 || direction.y <= 0.0) {
        return vec3(0.0);
    }
    float seconds = frame.water_clock / 50.0;
    float spacing = 1.0 / frame.lightning_rate;
    float slot = floor(seconds / spacing);
    uint h = sky_hash(uint(slot) * 747796405u + 2891336453u);
    if ((h & 3u) == 0u || (sky_hash(h) & 3u) != 0u) {
        return vec3(0.0);
    }
    float since = seconds - (slot + 0.1 + 0.6 * float((h >> 2) & 0xffffu) / 65536.0) * spacing;
    if (since < 0.0 || since > 0.45) {
        return vec3(0.0);
    }
    float stroke = exp(-since * 25.0) + 0.6 * step(0.12, since) * exp(-(since - 0.12) * 16.0);
    float turn = float(h >> 18) / 16384.0 * 6.28318530718;
    // Where the direction stands against the bolt: its turn round, and how
    // high, in radians.
    float round_off = atan(direction.z, direction.x) - turn;
    round_off = mod(round_off + 3.14159265, 6.28318530718) - 3.14159265;
    float high = asin(clamp(direction.y, 0.0, 1.0));
    const float TOP = 0.32;
    if (high > TOP || abs(round_off) > 0.25) {
        return vec3(0.0);
    }
    // The channel: eight straight runs, each its own lean.
    float along = high / TOP * 8.0;
    float run = floor(along);
    float offset = 0.0;
    for (int k = 0; k < 8; k++) {
        float lean = (float(sky_hash(h ^ uint(k * 7919 + 1)) & 0xffffu) / 65535.0 - 0.5) * 0.05;
        offset += lean * clamp(along - float(k), 0.0, 1.0);
    }
    float across = abs(round_off - offset) * cos(high);
    float core = exp(-across * across / (0.0012 * 0.0012));
    float halo = exp(-across * across / (0.008 * 0.008));
    // A fork from the middle run, now and then.
    if ((h & 0x40000u) != 0u && run >= 3.0) {
        float fork = abs(round_off - offset - (along - 3.0) * 0.018) * cos(high);
        core = max(core, 0.6 * exp(-fork * fork / (0.001 * 0.001)));
    }
    return (vec3(1.0) * core * 3.0 + frame.lightning_color * halo * 0.6) * min(stroke, 1.0);
}

#ifdef SKY_NOISE_BINDING
// The clouds' noise, tiling every texture unit (sky_noise.comp): the shape
// noise's red is Perlin-Worley, its green to alpha Worley at three
// frequencies; the detail noise's red to blue Worley at three more.
layout(set = 0, binding = SKY_NOISE_BINDING) uniform sampler3D sky_shape_noise;
layout(set = 0, binding = SKY_NOISE_BINDING + 1) uniform sampler3D sky_detail_noise;

// How much light a world unit of cloud at density one takes: enough that
// a cloud is opaque within a few hundred units, its edges sharp.
const float CLOUD_SIGMA = 0.01;
// The sunlit side's brightness against the world's light.
const float CLOUD_SUN = 1.6;
// How far along a view ray the march follows the layer past where it
// enters it: farther clouds are mostly haze.
const float CLOUD_REACH = 24000.0;
// The view's clouds thin within a few tens of units of the eye, to a third
// at this distance, so a wisp the eye flies into never fills the view in a
// frame; past it the eye sees a cloud's whole density, a whiteout inside
// one. It was 800 until 2026-10-29: a clear bubble about the eye, its wall
// backing away as one flew into it.
const float CLOUD_CLEAR = 50.0;
// The most optical depth the clouds between the eye and the craft it
// follows take (frame.cloud_craft), eased toward it: the craft stays at
// least 60% in sight inside a cloud, while all beyond it whites out.
const float CLOUD_FLOOR = 0.5;
// The craft's wake through the clouds (cloud_wake.rs): a tube this wide
// about its way where it has just passed, widening by WAKE_GROWTH a second
// as the cloud closes in, gone after WAKE_LIFE seconds.
const float WAKE_RADIUS = 120.0;
const float WAKE_GROWTH = 150.0;
const float WAKE_LIFE = 3.0;

float remap(float value, float low, float high, float to_low, float to_high) {
    return to_low + (clamp(value, low, high) - low) / max(high - low, 1.0e-5) * (to_high - to_low);
}

// How the layer's clouds run over its height `h` (0 at its base, 1 at its
// top), by their shape: the density's envelope (x), fading steadily toward
// the top, and how far the footprint's edge rises there (y), so a cloud's
// sides lean in to a rounded top. An envelope that held its strength up
// the layer drew each footprint straight up: walls and a flat top, the
// rectangular clouds of 2026-10-23.
// The world's `cloud_round` blends each shape from full-bodied (0: the
// envelope holds to mid-layer and the footprint stands straight) to domed
// (1).
vec2 cloud_shape(float h) {
    float shape = frame.cloud_shape;
    float domed = frame.cloud_round;
    if (shape < 0.5) {
        // Cumulus: flat bases, domed tops.
        return vec2(smoothstep(0.0, 0.07, h) * (1.0 - smoothstep(mix(0.45, 0.15, domed), 1.0, h)),
                    0.22 * domed);
    }
    if (shape < 1.5) {
        // Stratus: a thin sheet low in the layer.
        return vec2(smoothstep(0.0, 0.1, h) * (1.0 - smoothstep(mix(0.2, 0.15, domed), 0.5, h)),
                    0.08 * domed);
    }
    if (shape < 2.5) {
        // Storm: towers to the layer's top, narrowing as they climb.
        return vec2(smoothstep(0.0, 0.05, h) * (1.0 - smoothstep(mix(0.85, 0.4, domed), 1.0, h)),
                    0.16 * domed);
    }
    // Ash: thick and low.
    return vec2(smoothstep(0.0, 0.03, h) * (1.0 - smoothstep(mix(0.25, 0.2, domed), 0.9, h)),
                0.1 * domed);
}

// A field of `n` tiles across the wrap on Z as tiles a world unit on X and
// Z: stretched `cloud_streak` times across X, rounded to whole tiles.
vec2 cloud_tiles(float n) {
    return vec2(max(floor(n / frame.cloud_streak + 0.5), 1.0), n) / SKY_WRAP;
}

// The clouds' density at world point `world`, 0 to about 1, and the
// point's height `h` in its cloud's own layer (0 at its base, 1 at its
// top). `detail` erodes their edges with the finer noise (the view's
// march; the light's and the shadows' go without).
float cloud_density_at(vec3 world, bool detail, out float h) {
    h = 0.0;
    if (world.y <= frame.cloud_base - frame.cloud_rise
        || world.y >= frame.cloud_top + frame.cloud_rise) {
        return 0.0;
    }
    float thickness = max(frame.cloud_top - frame.cloud_base, 1.0);
    // Whole tiles across the world's wrap, so the sky meets itself there.
    float tiles = max(floor(SKY_WRAP / max(frame.cloud_size, 256.0)), 1.0);
    float weather_tiles = max(floor(tiles / 4.0), 1.0);
    vec2 drift = vec2(1.0, 0.6) * frame.cloud_speed * frame.water_clock;
    vec3 moved = vec3(world.x + drift.x, world.y, world.z + drift.y);
    // Each cloud stands a little higher or lower than the next, base and
    // top together, by up to the world's rise: a field as broad as the
    // clouds, smooth across the sky, so they no longer read as one flat
    // sheet. Its noise gathers about 0.485 (spread 0.11), which the lift
    // stretches to a whole rise either way.
    float lift = 0.0;
    if (frame.cloud_rise > 0.0) {
        vec2 lifted = moved.xz * cloud_tiles(weather_tiles);
        float n = textureLod(sky_shape_noise, vec3(lifted.x, 0.87, lifted.y), 0.0).g;
        lift = frame.cloud_rise * clamp((n - 0.485) / 0.19, -1.0, 1.0);
    }
    h = (world.y - frame.cloud_base - lift) / thickness;
    if (h <= 0.0 || h >= 1.0) {
        return 0.0;
    }
    // Where clouds stand: a broad field across X and Z, of which the
    // layer's cover shows, gathered by a broader one into groups with clear
    // sky between, so they do not stand in an even lattice.
    // Torn by the wind, they stretch across X, and they lean with it as
    // they climb.
    // Each field takes fewer tiles across X, but whole ones: a fraction
    // there jumped the sky as the eye crossed the wrap (Kankutan's streak 3).
    vec2 shear = vec2(0.35, 0.2) * (h * (frame.cloud_top - frame.cloud_base));
    vec2 broad = moved.xz * cloud_tiles(weather_tiles) + shear * (weather_tiles / SKY_WRAP);
    float weather = textureLod(sky_shape_noise, vec3(broad.x, 0.37, broad.y), 0.0).g;
    vec2 wide = moved.xz * cloud_tiles(max(floor(weather_tiles / 2.0), 1.0));
    float groups = textureLod(sky_shape_noise, vec3(wide.x, 0.71, wide.y), 0.0).g;
    weather *= mix(0.75, 1.15, smoothstep(0.4, 0.65, groups));
    // The noise gathers about its middle (spread about a tenth), so the
    // edge that leaves the layer's cover of the sky clouded lies near 0.5.
    // The field rises gently across it and even at its fullest leaves the
    // shape noise a third to carve: the clouds take their shapes in three
    // dimensions, not the field's two drawn upward. An overcast (cover past
    // 0.7) closes toward a full deck.
    vec2 shaped = cloud_shape(h);
    float edge = 0.5 + 0.3 * (0.5 - frame.cloud_cover) + shaped.y * h;
    float fullest = mix(0.7, 1.0, smoothstep(0.7, 1.0, frame.cloud_cover));
    float coverage = fullest * smoothstep(edge - 0.1, edge + 0.15, weather);
    if (coverage <= 0.0) {
        return 0.0;
    }
    vec2 across = cloud_tiles(tiles);
    vec3 p = moved * across.yyy;
    p.x = moved.x * across.x;
    vec4 shape = textureLod(sky_shape_noise, p, 0.0);
    float fbm = shape.g * 0.625 + shape.b * 0.25 + shape.a * 0.125;
    float base = remap(shape.r, fbm - 1.0, 1.0, 0.0, 1.0) * shaped.x;
    float density = remap(base, 1.0 - coverage, 1.0, 0.0, 1.0) * coverage;
    if (detail && density > 0.0) {
        vec3 fine = textureLod(sky_detail_noise, p * 6.0, 0.0).rgb;
        float wisps = fine.r * 0.625 + fine.g * 0.25 + fine.b * 0.125;
        // Wispy at the base, billowing higher up.
        float erode = mix(wisps, 1.0 - wisps, clamp(h * 4.0, 0.0, 1.0));
        density = remap(density, erode * 0.45, 1.0, 0.0, 1.0);
    }
    return max(density, 0.0);
}

float cloud_density(vec3 world, bool detail) {
    float h;
    return cloud_density_at(world, detail, h);
}

// How brightly the storm's veins glow at cloud point `world`, 0 to 1: thin
// sheets where the Worley noise crosses its middle, a web of light held
// inside the deck, flickering slowly as a broader noise drifts through it.
// Chosen 2026-10-23 (Tabuchi's first sheets) over winding
// filaments.
float cloud_vein(vec3 world) {
    float tiles = max(floor(SKY_WRAP / max(frame.cloud_size, 256.0)), 1.0);
    float time = frame.water_clock / 50.0;
    vec3 p = world * (tiles / SKY_WRAP) * 2.0;
    float n = textureLod(sky_detail_noise, p, 0.0).g;
    float vein = pow(1.0 - abs(2.0 * n - 1.0), 14.0);
    float flicker = textureLod(sky_shape_noise, p * 0.5 + vec3(0.0, time * 0.02, 0.0), 0.0).r;
    return vein * smoothstep(0.3, 0.8, flicker);
}

// How much the craft's wake has thinned the clouds at eye-relative point
// `p`, 0 to 1: the most of its runs between points, each a tube about its
// way, widening with its age (`r`), fading as (1 - age / life)^2, softly
// edged. `bound` (xyz and radius) holds them all.
float cloud_wake(vec3 p, vec4 bound) {
    vec3 off = p - bound.xyz;
    if (dot(off, off) > bound.w * bound.w) {
        return 0.0;
    }
    float hole = 0.0;
    for (int i = 0; i + 1 < int(frame.cloud_wake_count); i++) {
        vec4 a = frame.cloud_wake[i];
        vec4 b = frame.cloud_wake[i + 1];
        vec3 ab = b.xyz - a.xyz;
        float s = clamp(dot(p - a.xyz, ab) / max(dot(ab, ab), 1.0e-3), 0.0, 1.0);
        float age = mix(a.w, b.w, s);
        float r = WAKE_RADIUS + WAKE_GROWTH * age;
        float fade = max(1.0 - age / WAKE_LIFE, 0.0);
        hole = max(hole, fade * fade * (1.0 - smoothstep(0.35 * r, r, length(p - a.xyz - ab * s))));
    }
    return hole;
}

// The sphere holding the craft's wake, or none (radius 0) where it has
// none or the ray along `direction` from the eye passes outside it.
vec4 cloud_wake_bound(vec3 direction) {
    int count = int(frame.cloud_wake_count);
    if (count < 2) {
        return vec4(0.0);
    }
    vec3 middle = vec3(0.0);
    for (int i = 0; i < count; i++) {
        middle += frame.cloud_wake[i].xyz;
    }
    middle /= float(count);
    float reach = 0.0;
    for (int i = 0; i < count; i++) {
        vec4 point = frame.cloud_wake[i];
        reach = max(reach, length(point.xyz - middle) + WAKE_RADIUS + WAKE_GROWTH * point.w);
    }
    float along = max(dot(middle, direction), 0.0);
    return length(middle - direction * along) < reach ? vec4(middle, reach) : vec4(0.0);
}

// Henyey-Greenstein's phase for cosine `c`, with an average of one.
float cloud_phase(float c, float g) {
    float g2 = g * g;
    return (1.0 - g2) / pow(max(1.0 + g2 - 2.0 * g * c, 1.0e-4), 1.5);
}

// How much cloud lies toward the sun from cloud point `world`, in six
// growing steps.
float cloud_sun_depth(vec3 world) {
    float depth = 0.0;
    float step = (frame.cloud_top - frame.cloud_base) / 10.0;
    float t = 0.0;
    for (int i = 0; i < 6; i++) {
        float span = step * (1.0 + float(i) * 0.6);
        t += span;
        depth += cloud_density(world + frame.sun_direction * (t - 0.5 * span), false) * span;
    }
    return depth;
}

// The sunlight cloud point `world` scatters toward a view `facing` the sun
// by that cosine: Beer-Lambert through the cloud toward the sun, then
// Hillaire's octaves (2016) for the light scattered many times inside,
// each with half the share, half the depth's hold and half the lean of
// the phase (a forward lobe for the silver lining, a gentler backward
// one), which keeps thick clouds bright and white.
float cloud_sunlight(vec3 world, float facing) {
    if (frame.sun_direction.y <= 0.0) {
        return 0.0;
    }
    float depth = cloud_sun_depth(world) * CLOUD_SIGMA;
    float light = 0.0;
    float share = 1.0;
    // Each octave scatters on the albedo's part of the last: smog's light
    // dies inside it, a water cloud's carries through.
    float weight = 1.0;
    for (int octave = 0; octave < 4; octave++) {
        float phase = mix(cloud_phase(facing, 0.75 * share), cloud_phase(facing, -0.25 * share), 0.35);
        light += weight * min(phase, 6.0) * exp(-depth * share);
        share *= 0.5;
        weight *= 0.5 * frame.cloud_albedo;
    }
    return light;
}

// The clouds along `direction` from the eye, marched in about `steps`
// steps, each sampled `jitter` (0 to 1) into it: their light in rgb, and
// how much of what lies behind them shows in alpha. `placed` takes, in
// world units along the ray, how far the light beyond the near band comes
// from (x, 1e30 where there is none) and all the light's (w), and of the
// near band, the clouds within `near_reach` of the eye (the craft the
// camera follows; 0: none), how much it lets through (y) and its share of
// the light (z); the near band takes at most CLOUD_FLOOR of optical depth.
// Lit by the sun through their depth, darker at thin edges (the powder
// term), and by the sky above and the ground below; far ones fade into the
// horizon's haze. The march starts at the eye inside the layer, and near
// the eye its steps start at 12 units and grow by a twelfth of the
// distance, up to the even step across the layer, so a flight through a
// cloud whites out with its wisps sharp as they stream past.
vec4 clouds_toward(vec3 direction, int steps, float jitter, float near_reach, out vec4 placed) {
    vec4 none = vec4(0.0, 0.0, 0.0, 1.0);
    placed = vec4(1.0e30, 1.0, 0.0, 1.0e30);
    if (frame.cloud_cover <= 0.0) {
        return none;
    }
    // The layer with its clouds' rise either way.
    float low = frame.cloud_base - frame.cloud_rise;
    float high = frame.cloud_top + frame.cloud_rise;
    float enter = 0.0;
    float leave = CLOUD_REACH;
    if (abs(direction.y) >= 1.0e-4) {
        enter = (low - frame.eye.y) / direction.y;
        leave = (high - frame.eye.y) / direction.y;
        if (enter > leave) {
            float swap = enter;
            enter = leave;
            leave = swap;
        }
        enter = max(enter, 0.0);
        if (leave <= 0.0) {
            return none;
        }
        leave = min(leave, enter + CLOUD_REACH);
    } else if (frame.eye.y <= low || frame.eye.y >= high) {
        // A level ray stays at the eye's height: inside the layer or never.
        return none;
    }
    float even = (leave - enter) / float(steps);
    float facing = dot(direction, frame.sun_direction);
    // A flash lights the deck round a point in its direction, low in the
    // layer, a few thousand units out.
    vec4 flash = lightning_flash();
    vec3 flash_point = vec3(frame.eye.x + flash.x * 5000.0,
                            mix(frame.cloud_base, frame.cloud_top, 0.3),
                            frame.eye.z + flash.z * 5000.0);
    bool veins = max(frame.cloud_veins.r, max(frame.cloud_veins.g, frame.cloud_veins.b)) > 0.0;
    vec3 light = vec3(0.0);
    float through = 1.0;
    float weighed = 0.0;
    float weights = 0.0;
    float far_weighed = 0.0;
    float far_weights = 0.0;
    vec3 near_light = vec3(0.0);
    float near_through = 1.0;
    float near_depth = 0.0;
    vec4 wake = cloud_wake_bound(direction);
    float t = enter;
    // Far from the eye every step is the even one, as the march always
    // took; the short steps near it add at most about 60.
    for (int i = 0; i < steps + 64; i++) {
        float dt = min(max(t / 12.0, 12.0), even);
        float at = t + jitter * dt;
        t += dt;
        if (at >= leave) {
            break;
        }
        vec3 world = frame.eye + direction * at;
        float h;
        float density = cloud_density_at(world, true, h);
        if (density <= 0.002) {
            continue;
        }
        density *= 1.0 - exp(-at / CLOUD_CLEAR);
        if (wake.w > 0.0) {
            // The thin of it goes first, so the hole takes the cloud's shape.
            density = max(density - cloud_wake(direction * at, wake), 0.0);
        }
        float optical = density * CLOUD_SIGMA * dt;
        bool near = at < near_reach;
        if (near) {
            // The near band's depth eased toward CLOUD_FLOOR: its integral
            // of e^(-depth / floor).
            float eased = CLOUD_FLOOR * (exp(-near_depth / CLOUD_FLOOR)
                                         - exp(-(near_depth + optical) / CLOUD_FLOOR));
            near_depth += optical;
            optical = eased;
        }
        float powder = 1.0 - exp(-density * 4.0);
        vec3 sunlit = CLOUD_SUN * frame.cloud_lit * cloud_sunlight(world, facing)
            * mix(0.5, 1.0, powder);
        vec3 ambient = mix(frame.cloud_shade, mix(frame.cloud_shade, frame.sky_zenith, 0.5), h)
            // Light from below (lava, Artoria's clarifiers) on the undersides,
            // fading up through the layer.
            + frame.cloud_under * (1.5 * (1.0 - h) * (1.0 - h));
        // What glows inside: the storm's veins and a flash.
        vec3 glow = veins ? frame.cloud_veins * cloud_vein(world) : vec3(0.0);
        if (flash.w > 0.0) {
            glow += frame.lightning_color * (3.0 * flash.w)
                * exp(-length(world - flash_point) / 3000.0);
        }
        float absorbed = exp(-optical);
        float share = through * (1.0 - absorbed);
        vec3 lit = share * (sunlit + ambient + glow);
        light += lit;
        weighed += share * at;
        weights += share;
        through *= absorbed;
        if (near) {
            near_light += lit;
            near_through = through;
        } else {
            far_weighed += share * at;
            far_weights += share;
        }
        if (through < 0.01) {
            break;
        }
    }
    float haze = exp(-enter / frame.cloud_haze);
    placed = vec4(far_weights > 1.0e-4 ? far_weighed / far_weights : 1.0e30,
                  1.0 - (1.0 - near_through) * haze,
                  clamp(dot(near_light, vec3(1.0)) / max(dot(light, vec3(1.0)), 1.0e-6), 0.0, 1.0),
                  weights > 1.0e-4 ? weighed / weights : 1.0e30);
    return vec4(light * haze, 1.0 - (1.0 - through) * haze);
}
#endif

#ifdef SKY_SHADOW_BINDING
// The clouds' shadow map (pt_sky_clouds.comp pass 2): how much sunlight
// passes the layer above each point of its base plane, SKY_SHADOW_SPAN
// across about the eye.
layout(set = 0, binding = SKY_SHADOW_BINDING) uniform sampler2D cloud_shadows;

// How much sunlight reaches an eye-relative point under the clouds: where
// its ray toward the sun crosses their layer's lowest base (the layer's
// less its clouds' rise), the shadow map's light, of which
// frame.cloud_shadow is taken; inside the layer, only the share of the
// layer above the point shades it. The shadows drift with the clouds.
float cloud_light(vec3 position) {
    vec3 world = frame.eye + position;
    float low = frame.cloud_base - frame.cloud_rise;
    float high = frame.cloud_top + frame.cloud_rise;
    if (frame.cloud_shadow <= 0.0 || frame.cloud_cover <= 0.0 || frame.sun_direction.y <= 0.05
        || world.y >= high) {
        return 1.0;
    }
    float rise = low - world.y;
    vec2 base = world.xz + frame.sun_direction.xz * (rise / frame.sun_direction.y);
    vec2 uv = (base - frame.eye.xz) / SKY_SHADOW_SPAN + 0.5;
    if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) {
        return 1.0;
    }
    float above = 1.0 - clamp(-rise / (high - low), 0.0, 1.0);
    float light = pow(max(textureLod(cloud_shadows, uv, 0.0).r, 1.0e-4), above);
    return mix(1.0, light, frame.cloud_shadow);
}
#endif
