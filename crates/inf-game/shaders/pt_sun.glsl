// The sun's specular off a surface, shared by the tracer (pt_trace.comp)
// and the raster's wet ground (vk_mirror.comp); the sun's light is
// pt_frame.glsl's sun_light. Include after pt_frame.glsl.

// Schlick's Fresnel.
vec3 fresnel(vec3 f0, float cosine) {
    return f0 + (1.0 - f0) * pow(1.0 - clamp(cosine, 0.0, 1.0), 5.0);
}

// The sun's specular light off a surface facing `normal`, seen along
// `view`, in light_of's units (the BSDF times pi): GGX with height-correlated
// Smith masking and Schlick's Fresnel from `f0`, its roughness widened by the
// sun's disc and renormalised (Karis), so a smooth surface shows the disc and
// not a point.
vec3 sun_specular(vec3 normal, vec3 view, vec3 sun, float roughness, vec3 f0) {
    float n_l = dot(normal, sun);
    float n_v = dot(normal, -view);
    if (n_l <= 0.0 || n_v <= 0.0) {
        return vec3(0.0);
    }
    vec3 half_way = normalize(sun - view);
    float n_h = max(dot(normal, half_way), 0.0);
    float alpha = max(roughness * roughness, 1.0e-3);
    float disc = sqrt(max(1.0 - frame.shadow_cone * frame.shadow_cone, 0.0));
    float wide = min(alpha + 0.5 * disc, 1.0);
    float w2 = wide * wide;
    float d = w2 / (3.14159265 * pow(n_h * n_h * (w2 - 1.0) + 1.0, 2.0))
        * (alpha / wide) * (alpha / wide);
    float a2 = alpha * alpha;
    float masked = n_l * sqrt(n_v * n_v * (1.0 - a2) + a2) + n_v * sqrt(n_l * n_l * (1.0 - a2) + a2);
    float visibility = 0.5 / max(masked, 1.0e-5);
    return 3.14159265 * d * visibility * n_l * fresnel(f0, dot(half_way, -view));
}
