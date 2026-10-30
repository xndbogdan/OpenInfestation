#version 450
// Thermal vision's look over the whole world view, before the HUD: the
// sensor's grey brought back up smoothly, with an uncooled camera's
// character at the sensor's own resolution. Edges are sharpened and the
// image's fine contrast added back (detail enhancement), hot things glow
// into a halo, the sensor's faint fixed column and row stripes show, and a
// grain changes every frame, strongest where the scene is cold and flat but
// never enough to hide a small warm target. A light vignette darkens the
// corners. The grey then shows white hot, black and white only, or ironbow.
layout(set = 0, binding = 0) uniform sampler2D sensor_image;
layout(push_constant) uniform Camera {
    // The sensor's size in pixels.
    vec2 size;
    float frame;
    float grain;
    float stripes;
    float sharpen;
    float halo;
    float vignette;
    // The view's width over its height.
    float aspect;
    // The detail enhancement's strength, and ironbow (1) or white hot (0).
    // (Floats only, so the block packs as the host writes it.)
    float detail;
    float ironbow;
} camera;
layout(location = 0) in vec2 v_texture;
layout(location = 0) out vec4 o_color;

uint hash(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}

// A value in -0.5..0.5 for `x` and `seed`.
float uniform_noise(uint x, uint seed) {
    return float(hash(x ^ hash(seed)) >> 8) / 16777216.0 - 0.5;
}

float grey_at(vec2 uv) {
    return texture(sensor_image, uv).r;
}

// Ironbow: black through purple, red and orange to yellow and white.
vec3 ironbow(float t) {
    const vec3 stops[6] = vec3[6](vec3(0.0), vec3(0.3, 0.0, 0.45), vec3(0.75, 0.05, 0.35),
                                  vec3(0.95, 0.45, 0.05), vec3(1.0, 0.85, 0.2), vec3(1.0));
    float at = clamp(t, 0.0, 1.0) * 5.0;
    int low = min(int(at), 4);
    return mix(stops[low], stops[low + 1], at - float(low));
}

void main() {
    vec2 texel = 1.0 / camera.size;
    float raw = grey_at(v_texture);
    float grey = raw;
    // Sharpened against the sensor's four neighbours.
    float around = 0.25 * (grey_at(v_texture + vec2(texel.x, 0.0))
                           + grey_at(v_texture - vec2(texel.x, 0.0))
                           + grey_at(v_texture + vec2(0.0, texel.y))
                           + grey_at(v_texture - vec2(0.0, texel.y)));
    grey += camera.sharpen * (grey - around);
    // The fine contrast a few sensor pixels round, added back, and the glow
    // of hot things there.
    float glow = 0.0;
    float wide = 0.0;
    for (int i = 0; i < 8; i++) {
        float angle = 0.785398163 * float(i);
        float there = grey_at(v_texture + vec2(cos(angle), sin(angle)) * texel * 3.0);
        glow += max(there - 0.6, 0.0);
        wide += there;
    }
    grey += camera.detail * (raw - wide / 8.0);
    grey += camera.halo * glow / 8.0;
    // The sensor's fixed pattern, then this frame's grain.
    uvec2 pixel = uvec2(v_texture * camera.size);
    grey += camera.stripes * (uniform_noise(pixel.x, 17u) + 0.5 * uniform_noise(pixel.y, 29u));
    uint seed = uint(camera.frame) * 0x9e3779b9u;
    float noise = uniform_noise(pixel.x + pixel.y * 4099u, seed)
        + uniform_noise(pixel.x * 7919u + pixel.y, seed + 1u);
    float cold = 1.0 - clamp(grey, 0.0, 1.0);
    grey += camera.grain * (0.4 + 0.6 * cold) * noise;
    vec2 centred = (v_texture - 0.5) * vec2(camera.aspect, 1.0);
    grey = clamp(grey * (1.0 - camera.vignette * dot(centred, centred)), 0.0, 1.0);
    o_color = vec4(camera.ironbow > 0.5 ? ironbow(grey) : vec3(grey), 1.0);
}
