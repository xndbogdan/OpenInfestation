// SMAA, Jimenez et al.'s Enhanced Subpixel Morphological Antialiasing, as
// compute passes: the reference implementation (smaa/SMAA.hlsl, unmodified;
// smaa/README.md) through its own language hooks, sampling at the top
// level, as compute shaders must. Its edge pass discards a pixel without
// edges; here that pixel writes no edges. Include it after the pass's
// bindings.

layout(push_constant) uniform Push {
    // 1/width, 1/height, width, height.
    vec4 metrics;
} push;

#define SMAA_RT_METRICS push.metrics
#define SMAA_CUSTOM_SL 1
#define SMAA_PRESET_HIGH 1
#define SMAATexture2D(tex) sampler2D tex
#define SMAATexturePass2D(tex) tex
#define SMAASampleLevelZero(tex, coord) textureLod(tex, coord, 0.0)
#define SMAASampleLevelZeroPoint(tex, coord) textureLod(tex, coord, 0.0)
#define SMAASampleLevelZeroOffset(tex, coord, offset) textureLodOffset(tex, coord, 0.0, offset)
#define SMAASample(tex, coord) textureLod(tex, coord, 0.0)
#define SMAASamplePoint(tex, coord) textureLod(tex, coord, 0.0)
#define SMAASampleOffset(tex, coord, offset) textureLodOffset(tex, coord, 0.0, offset)
#define SMAA_FLATTEN
#define SMAA_BRANCH
#define lerp(a, b, t) mix(a, b, t)
#define saturate(a) clamp(a, 0.0, 1.0)
#define mad(a, b, c) fma(a, b, c)
#define SMAAGather(tex, coord) textureGather(tex, coord)
#define float2 vec2
#define float3 vec3
#define float4 vec4
#define int2 ivec2
#define int3 ivec3
#define int4 ivec4
#define bool2 bvec2
#define bool3 bvec3
#define bool4 bvec4
#define discard return float2(0.0, 0.0)
#include "smaa/SMAA.hlsl"
#undef discard

// The pixel this invocation shades, or false past the image.
bool smaa_pixel(out ivec2 pixel, out vec2 texcoord) {
    pixel = ivec2(gl_GlobalInvocationID.xy);
    texcoord = (vec2(pixel) + 0.5) * push.metrics.xy;
    return all(lessThan(pixel, ivec2(push.metrics.zw)));
}
