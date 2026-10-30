// The rain's wet (world_weather.rs) on what it soaks, shared by the tracer
// (pt_trace.comp), the raster's wet ground (vk_mirror.comp, both through
// pt_ground.glsl) and the raster's metal (vk_metal.glsl). Include after
// pt_frame.glsl.
#ifndef PT_WET
#define PT_WET

// How much darker the rain makes the ground as it soaks it, all over.
const float RAIN_DARKEN = 0.28;

// The models' wet, the more on the faces turned up (`up`, the normal's
// height): a metal mirrors more and smoother; a sprite's bare surface (the
// tyres' rubber, BARE), no paint over it, darkens as the ground does as it
// is `soaked` and smooths, a faint mirror of its own dark colour; paint
// takes a faint gloss, a clear layer (`coat`) whose reflection is not its
// colour.
void wet_model(float up, bool bare, inout float metal, inout float rough, out float soaked,
               out bool coat) {
    float wet_coat = frame.wet_models * (0.55 + 0.45 * max(up, 0.0));
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
