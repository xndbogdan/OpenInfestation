#version 450
// Where a moving model is what the world shows (its depth passes against
// the world's), its own motion, and in alpha that it is there.
layout(location = 0) in vec2 v_motion;
layout(location = 1) in float v_depth;
layout(location = 0) out vec4 o_motion;
void main() {
    if (v_depth > 1.0) discard;
    gl_FragDepth = clamp(v_depth, 0.0, 1.0);
    o_motion = vec4(v_motion, 0.0, 1.0);
}
