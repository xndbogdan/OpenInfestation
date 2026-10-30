// The water a blast throws up (scene_spray.rs), which the trace and the
// depth pass share: its sources, then their drops, eight floats each. A
// source: the sphere holding its drops, then its first drop's index and how
// many. A drop: its centre and radius, how it draws out along its motion
// (along that direction it is one plus the vector's length times as long),
// and what it is: 0 clear water, 1 white water. Include it after
// pt_frame.glsl; its binding is the trace set's.

struct SprayRecord {
    vec4 a;
    vec4 b;
};
layout(set = 0, binding = 22, scalar) readonly buffer Spray { SprayRecord spray[]; };

// The nearest drop the ray from the eye along `direction` meets before
// `reach` (then its distance): its index, or -1. A drop is never smaller
// than about a render pixel across, so the farthest still glint rather than
// come and go; `coverage` is the share of that it truly fills.
int spray_hit(vec3 direction, inout float reach, out vec3 normal, out float coverage) {
    int hit = -1;
    normal = vec3(0.0, 1.0, 0.0);
    coverage = 1.0;
    float pixel = frame.native.x / (frame.target.x * frame.focal.x);
    for (int s = 0; s < int(frame.spray_count); s++) {
        vec4 bound = spray[s].a;
        float b = dot(bound.xyz, direction);
        float h = b * b - (dot(bound.xyz, bound.xyz) - bound.w * bound.w);
        if (h <= 0.0 || b + sqrt(h) < frame.near || b - sqrt(h) > reach) {
            continue;
        }
        int first = int(spray[s].b.x);
        int last = first + int(spray[s].b.y);
        for (int i = first; i < last; i++) {
            vec4 drop = spray[i].a;
            float radius = max(drop.w, 0.7 * length(drop.xyz) * pixel);
            // In the drop's own frame, squeezed along its motion to a ball.
            vec3 drawn = spray[i].b.xyz;
            float drawn2 = dot(drawn, drawn);
            vec3 o = -drop.xyz;
            vec3 d = direction;
            vec3 along = vec3(0.0);
            float squeeze = 0.0;
            if (drawn2 > 1.0e-6) {
                float stretch = 1.0 + sqrt(drawn2);
                along = drawn / (stretch - 1.0);
                squeeze = 1.0 - 1.0 / stretch;
                o -= along * dot(o, along) * squeeze;
                d -= along * dot(d, along) * squeeze;
            }
            float a = dot(d, d);
            float half_b = dot(o, d);
            float disc = half_b * half_b - a * (dot(o, o) - radius * radius);
            if (disc <= 0.0) {
                continue;
            }
            float t = (-half_b - sqrt(disc)) / a;
            if (t < frame.near || t >= reach) {
                continue;
            }
            reach = t;
            hit = i;
            vec3 local = o + d * t;
            normal = normalize(local - along * dot(local, along) * squeeze);
            coverage = drop.w * drop.w / (radius * radius);
        }
    }
    return hit;
}
