#version 450
// Thermal vision's sensor: the world as the frame holds it after its last
// world draw (the traced world as heat, the raster's effects over it, or
// retail's infrared image without the tracer), brought down to the camera's
// own, lower resolution as one grey: the brightest channel, which keeps
// retail's green infrared and the tracer's grey alike. Each sensor pixel
// averages its footprint with a grid of filtered taps.
layout(set = 0, binding = 0) uniform sampler2D frame_image;
layout(push_constant) uniform Sensor {
    // The sensor's size in pixels.
    vec2 size;
} sensor;
layout(location = 0) in vec2 v_texture;
layout(location = 0) out vec4 o_color;

void main() {
    vec2 footprint = 1.0 / sensor.size;
    vec2 corner = v_texture - 0.5 * footprint;
    float sum = 0.0;
    for (int y = 0; y < 4; y++) {
        for (int x = 0; x < 4; x++) {
            vec3 rgb = texture(frame_image, corner + (vec2(x, y) + 0.5) * 0.25 * footprint).rgb;
            sum += max(rgb.r, max(rgb.g, rgb.b));
        }
    }
    o_color = vec4(vec3(sum / 16.0), 1.0);
}
