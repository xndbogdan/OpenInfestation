// A glossy lobe's shape, which the denoiser's passes read (pt_temporal.comp,
// pt_atrous.comp): the path pass's glossy bounce samples GGX's distribution
// of linear roughness `rough` (its alpha the square), as the raster's metal
// does (vk_metal.glsl).

// A direction about `normal` reflecting `view` off a microfacet drawn from
// GGX's distribution of `roughness` (its alpha the square): a metal's
// reflection, blurred as rough as it is.
vec3 glossy_direction(vec3 view, vec3 normal, float roughness, vec2 xi, out float weight) {
    // GGX's visible normals (Heitz 2018): the facets the view sees, in
    // proportion as it sees them, so the sample's weight is Smith's masking
    // of the reflected ray alone (the Fresnel term rides on the albedo the
    // resolve multiplies by).
    float alpha = max(roughness * roughness, 1.0e-3);
    vec3 tangent = normalize(abs(normal.y) < 0.99 ? cross(normal, vec3(0.0, 1.0, 0.0))
                                                  : cross(normal, vec3(1.0, 0.0, 0.0)));
    vec3 bitangent = cross(normal, tangent);
    vec3 v = vec3(dot(-view, tangent), dot(-view, bitangent), max(dot(-view, normal), 1.0e-4));
    vec3 stretched = normalize(vec3(alpha * v.x, alpha * v.y, v.z));
    float across = stretched.x * stretched.x + stretched.y * stretched.y;
    vec3 t1 = across > 0.0 ? vec3(-stretched.y, stretched.x, 0.0) / sqrt(across)
                           : vec3(1.0, 0.0, 0.0);
    vec3 t2 = cross(stretched, t1);
    float radius = sqrt(xi.x);
    float phi = 6.28318530718 * xi.y;
    float p1 = radius * cos(phi);
    float p2 = radius * sin(phi);
    float lean = 0.5 * (1.0 + stretched.z);
    p2 = (1.0 - lean) * sqrt(max(1.0 - p1 * p1, 0.0)) + lean * p2;
    vec3 seen_facet = p1 * t1 + p2 * t2 + sqrt(max(1.0 - p1 * p1 - p2 * p2, 0.0)) * stretched;
    vec3 facet = normalize(vec3(alpha * seen_facet.x, alpha * seen_facet.y, max(seen_facet.z, 0.0)));
    vec3 l = reflect(-v, facet);
    if (l.z <= 0.0) {
        weight = 0.0;
        return normal;
    }
    float a2 = alpha * alpha;
    weight = 2.0 * l.z / (l.z + sqrt(a2 + (1.0 - a2) * l.z * l.z));
    return normalize(tangent * l.x + bitangent * l.y + normal * l.z);
}

// How far along its reflected ray a glossy lobe's light seems to lie, as a
// share of the ray's length: 1 for a mirror, whose reflection lies as far
// behind it as the reflected point lies in front, falling to 0 as roughness
// spreads the lobe until its light no longer moves with the view (the
// specular dominant direction's lerp factor, Lagarde and de Rousiers,
// "Moving Frostbite to PBR", 2014).
float dominant_share(float rough) {
    float s = 1.0 - rough;
    return s * (sqrt(s) + rough);
}

// The half-width of the reflected light a lobe gathers, in radians: twice
// the angle round the normal holding three quarters of GGX's half vectors
// (its share within tan^2 = alpha^2 p / (1 - p)), as a half vector turned by
// an angle turns the reflection by twice it.
float lobe_width(float rough) {
    return 2.0 * atan(rough * rough * sqrt(3.0));
}

// How many pixels across the reflection of a point `reach` beyond a surface
// `distance` from the eye spreads, at half-width: the lobe's width there,
// seen from the eye along the reflected ray (`focal`: pixels a radian).
float lobe_footprint(float rough, float distance, float reach, float focal) {
    return reach * tan(min(lobe_width(rough), 1.4)) / (distance + reach) * focal;
}

// Whether a pixel has a glossy lobe this frame rides in the sign of the
// diffuse layers' alpha (the history's frames, the filter's sun
// visibility), as -(value + 1): the filter's passes read it with the
// diffuse light they load anyway, and touch the glossy layers only where a
// pixel has one.
float flagged(float value, bool glossy) {
    return glossy ? -(value + 1.0) : value;
}

float unflagged(float alpha) {
    return alpha < 0.0 ? -alpha - 1.0 : alpha;
}
