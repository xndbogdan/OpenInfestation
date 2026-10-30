// Where a primary hit was in the last traced frame (pt_temporal.comp): its
// point, carried by its model's own motion, through the last frame's view
// and the world's 16-bit wrap, and the four texels round it there that hold
// the same surface (their distance and normal agree), each with its
// bilinear weight. Include it after pt_frame.glsl, with HISTORY_GEOMETRY
// (the last frame's geometry image) and HISTORY_MOTION (this frame's object
// motion image) defined.

struct History {
    ivec2 base;
    vec4 weights;
    float total;
};

// The four texels round where `relative` (a point from the last frame's
// eye, within the wrap) lay in the last frame's view that hold a surface
// `expected` from that eye and facing as `normal` does.
History history_at(vec3 relative, float expected, vec3 normal, ivec2 size) {
    History history;
    history.base = ivec2(0);
    history.weights = vec4(0.0);
    history.total = 0.0;
    vec3 view = vec3(dot(relative, frame.previous_right), dot(relative, frame.previous_up),
                     dot(relative, frame.previous_forward));
    if (view.z <= frame.near) {
        return history;
    }
    vec2 native = frame.previous_centre + frame.previous_focal * vec2(view.x, -view.y) / view.z;
    vec2 at = (native + 0.5) * frame.target / frame.native - 0.5;
    history.base = ivec2(floor(at));
    vec2 f = at - vec2(history.base);
    for (int i = 0; i < 4; i++) {
        ivec2 tap = history.base + ivec2(i & 1, i >> 1);
        if (any(lessThan(tap, ivec2(0))) || any(greaterThanEqual(tap, size))) {
            continue;
        }
        vec4 then = imageLoad(HISTORY_GEOMETRY, tap);
        if (then.w <= 0.0 || abs(then.w - expected) > 0.03 * expected + 2.0
            || dot(then.xyz, normal) < 0.9) {
            continue;
        }
        float w = ((i & 1) == 1 ? f.x : 1.0 - f.x) * ((i >> 1) == 1 ? f.y : 1.0 - f.y);
        history.weights[i] = w;
        history.total += w;
    }
    return history;
}

// From the last frame's eye to where the point `offset` from this frame's
// eye was then, carried by `motion`, within the world's 16-bit wrap.
vec3 from_previous_eye(vec3 offset, vec3 motion) {
    vec3 relative = frame.eye + offset + motion - frame.previous_eye;
    relative.xz = mod(relative.xz + 32768.0, 65536.0) - 32768.0;
    return relative;
}

History history_taps(ivec2 pixel, vec4 here, ivec2 size) {
    if (frame.history_valid < 0.5) {
        History none;
        none.base = ivec2(0);
        none.weights = vec4(0.0);
        none.total = 0.0;
        return none;
    }
    vec3 relative = from_previous_eye(pixel_direction(vec2(pixel)) * here.w,
                                      imageLoad(HISTORY_MOTION, pixel).xyz);
    return history_at(relative, length(relative), here.xyz, size);
}

// Whether a primary hit at `pixel` (`here`: its geometry) has a history to
// follow: what the temporal pass takes from the last frame.
bool has_history(ivec2 pixel, vec4 here, ivec2 size) {
    return history_taps(pixel, here, size).total > 0.01;
}
