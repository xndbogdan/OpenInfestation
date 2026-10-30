// The struck crystals' arcs (crystal_flares.rs), shared by the tracer's
// composite (pt_composite.frag) and the raster's modern sky (vk_sky.comp):
// both lay them over the world from the frame block's sparks.

uint field_hash(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}

// The nearest the view ray `direction` (from the eye, unit length) passes
// the segment from `a` to `b`: that distance, and how far along the ray.
vec2 ray_segment(vec3 direction, vec3 a, vec3 b) {
    vec3 ab = b - a;
    float along_ab = dot(direction, ab);
    float apart = dot(ab, ab) - along_ab * along_ab;
    float s = apart > 1.0e-6
        ? clamp((dot(direction, a) * along_ab - dot(ab, a)) / apart, 0.0, 1.0)
        : 0.0;
    vec3 nearest = a + ab * s;
    float t = max(dot(direction, nearest), 0.0);
    return vec2(length(nearest - direction * t), t);
}

// A struck crystal's arcs, as games draw electricity (restrained, a
// blue aura round them, snappy like the flash): a few short bolts, each
// jumping between a pair of points on the crystal's own faces (the clumps'
// middle is air between their shards) and bowing a little off them,
// jagged but pinned at its ends; each lives a moment and another
// takes its place, so no more than ARC_SLOTS show at once; a hairline
// white core in a soft blue glow and a faint wider bloom. They burst with
// the strike and are gone with its flash.
const int ARC_SLOTS = 3;
const int ARC_RUNS = 8;
// How long a bolt lives before the next takes its slot, in the spark's
// seconds.
const float ARC_LIFE = 0.09;
// Half the width of a bolt's core, its glow and its bloom, in radians of
// the view: the core a pixel at 1080p at any distance.
const float ARC_CORE = 0.0008;
const float ARC_GLOW = 0.0045;
const float ARC_BLOOM = 0.02;

// A unit direction from three hashed bytes of `h`.
vec3 arc_direction(uint h) {
    float z = 2.0 * float(h & 0xffu) / 255.0 - 1.0;
    float turn = 6.28318530718 * float((h >> 8) & 0xffu) / 255.0;
    float ring = sqrt(max(1.0 - z * z, 0.0));
    return vec3(ring * cos(turn), z, ring * sin(turn));
}

// One bolt's run from `a` to `b` (its nearest pass to the view ray), added
// to the core, glow and bloom it lights at `weight`, if nothing nearer
// than `reach` hides it.
void arc_run(vec3 direction, vec3 a, vec3 b, float reach, float weight, inout vec3 lit) {
    vec2 near = ray_segment(direction, a, b);
    if (near.y <= frame.near || near.y >= reach) {
        return;
    }
    float off = near.x / near.y;
    lit = max(lit, weight * vec3(exp(-off * off / (ARC_CORE * ARC_CORE)),
                                 exp(-off * off / (ARC_GLOW * ARC_GLOW)),
                                 exp(-off * off / (ARC_BLOOM * ARC_BLOOM))));
}

// The struck crystals' arcs along the view `direction`, in front of the
// world at `reach` (along the view: whatever stands in front, the craft
// against the crystal among it, hides them), and a faint blue haze round
// each crystal while they play. Additive.
vec3 crystal_arcs(vec3 direction, float reach) {
    vec3 light = vec3(0.0);
    if (frame.thermal > 0.5) {
        return light;
    }
    for (int i = 0; i < int(frame.spark_count); i++) {
        vec3 middle = frame.spark_middle[i].xyz;
        float age = frame.spark_at[i].w;
        float radius = frame.spark_radius[i];
        // The burst: whole at the strike, half gone in a tenth of the
        // spark's second, out by six tenths.
        float burst = exp(-age * 6.0) * (1.0 - smoothstep(0.35, 0.6, age));
        float aside = length(cross(direction, middle));
        if (age < 0.0 || burst <= 0.0 || dot(direction, middle) < -radius
            || aside > 1.1 * radius) {
            continue;
        }
        float hidden = reach;
        vec3 lit = vec3(0.0);
        for (int k = 0; k < ARC_SLOTS; k++) {
            // Each slot's bolts, a third of a life apart from the next's.
            float local = age / ARC_LIFE + float(k) / float(ARC_SLOTS);
            uint h = field_hash(uint(floor(local)) * 2654435761u + uint(i * 31 + k) * 0x9e3779b9u);
            // Now and then a slot rests, so the bolts crackle.
            if ((h & 3u) == 0u && local > 1.0) {
                continue;
            }
            // Each bolt pops and fades over its moment.
            float weight = exp(-fract(local) * 2.5) * (0.7 + 0.3 * float(h >> 29) / 7.0);
            // Between a pair of points on the crystal's faces; the first slot's
            // first bolt from where the shot struck.
            int pair = (k == 0 && local < 1.0) ? 0 : int((h >> 4) % uint(ARC_PAIRS));
            vec3 a = frame.spark_anchors[(i * ARC_PAIRS + pair) * 2].xyz;
            vec3 b = frame.spark_anchors[(i * ARC_PAIRS + pair) * 2 + 1].xyz;
            float span = length(b - a);
            vec3 out_way = (a + b) * 0.5 - middle;
            out_way = dot(out_way, out_way) > 1.0e-4 ? normalize(out_way) : vec3(0.0, 1.0, 0.0);
            vec3 from = a;
            for (int r = 1; r <= ARC_RUNS; r++) {
                float s = float(r) / float(ARC_RUNS);
                // Bowing a little off the faces, jagged, pinned at both ends.
                float bow = sin(3.14159265 * s);
                uint j = field_hash(h + uint(r) * 0x85ebca6bu);
                vec3 to = mix(a, b, s) + out_way * 0.15 * span * bow
                    + (r < ARC_RUNS ? arc_direction(j) * 0.16 * span * bow : vec3(0.0));
                arc_run(direction, from, to, hidden, weight, lit);
                // One bolt in two forks once, faint and short, near its middle.
                if (r == ARC_RUNS / 2 && (h & 0x40u) != 0u) {
                    vec3 fork = to + (arc_direction(j ^ 0x27d4eb2fu) * 0.6 + out_way * 0.4)
                        * 0.3 * span;
                    arc_run(direction, to, fork, hidden, 0.5 * weight, lit);
                }
                from = to;
            }
        }
        vec3 tint = frame.spark_light[i].rgb;
        // The haze round the crystal, where it shows.
        float haze = dot(direction, middle) < hidden
            ? exp(-aside * aside / (0.36 * radius * radius)) * 0.05
            : 0.0;
        light += (vec3(0.85, 0.93, 1.0) * 2.5 * lit.x + tint * (0.75 * lit.y + 0.14 * lit.z + haze))
            * burst;
    }
    return light;
}

// The light a struck crystal throws on what lies round it, at eye-relative
// `point`, over the raster's world as `shown` (vk_sky.comp): as the
// tracer's lone glow at its middle, the flare's `under` share (0.55) of its
// light within its reach (650, modern_tuning.json's flare), fading as the
// flare does, linearly over its 1.5 spark seconds a second (crystal_flares.rs).
vec3 crystal_underglow(vec3 point, vec3 shown) {
    vec3 light = vec3(0.0);
    for (int i = 0; i < int(frame.spark_count); i++) {
        float age = frame.spark_at[i].w;
        vec3 offset = point - frame.spark_middle[i].xyz;
        float d2 = dot(offset, offset) / (650.0 * 650.0);
        if (age < 0.0 || d2 >= 1.0) {
            continue;
        }
        float flare = max(1.0 - age / (1.5 * frame.spark_seconds), 0.0)
            * (1.0 - smoothstep(0.7, 1.0, age / frame.spark_seconds));
        light += frame.spark_light[i].rgb * (0.55 * flare * (1.0 - d2));
    }
    return (shown + 0.05) * light;
}
