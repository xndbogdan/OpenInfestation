// The rain's wet (world_weather.rs) on what it soaks, shared by the tracer
// (pt_trace.comp), the raster's wet ground (vk_mirror.comp, both through
// pt_ground.glsl) and the raster's metal (vk_metal.glsl). Include after
// pt_frame.glsl.
#ifndef PT_WET
#define PT_WET

// How much darker the rain makes the ground as it soaks it, all over.
const float RAIN_DARKEN = 0.28;

// How wet the craft the camera follows is at eye-relative `position`
// (craft_wet.rs): under its wet line, near it; the wet line's edge soft.
float craft_wetness(vec3 position) {
    if (frame.craft_wet.y <= 0.0
        || distance(position, frame.craft_wet_at.xyz) > frame.craft_wet_at.w) {
        return 0.0;
    }
    return frame.craft_wet.y
        * (1.0 - smoothstep(frame.craft_wet.x - 6.0, frame.craft_wet.x + 6.0, position.y));
}

// The models' wet, `wet` of it (the rain's, or the water's on the craft),
// the more on the faces turned up (`up`, the normal's height): a metal
// mirrors more and smoother; a sprite's bare surface (the tyres' rubber,
// BARE), no paint over it, darkens as the ground does as it is `soaked`
// and smooths, a faint mirror of its own dark colour; paint takes a faint
// gloss, a clear layer (`coat`) whose reflection is not its colour.
void wet_model(float wet, float up, bool bare, inout float metal, inout float rough,
               out float soaked, out bool coat) {
    float wet_coat = wet * (0.55 + 0.45 * max(up, 0.0));
    soaked = 0.0;
    coat = false;
    if (metal > 0.0) {
        metal = min(metal + 0.3 * wet_coat, 0.95);
        rough *= 1.0 - 0.55 * wet_coat;
    } else if (bare) {
        metal = 0.3 * wet_coat;
        rough *= 1.0 - 0.55 * wet_coat;
        soaked = wet_coat;
    } else {
        metal = 0.12 * wet_coat;
        rough = 0.3;
        coat = true;
    }
}

#endif
