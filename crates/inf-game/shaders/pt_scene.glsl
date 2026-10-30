// The traced scene's model geometry, which the trace and the depth pass
// share: the frame's triangles, their vertices and sprites, and the test
// that lets rays through a masked sprite's transparent texels. Include it
// after pt_frame.glsl; its bindings are the trace set's.

struct Vertex {
    vec3 position;
    vec3 normal;
    vec2 uv;
    uint color;
    // Retail's vertex light, a shade of 128 is one (alpha 0: none).
    uint shade;
    // Retail's vertex fog: the far colour, and in alpha how much.
    uint fog;
    // Where the point was in the last frame, less where it is (its model's
    // own motion; zero for what keeps still).
    vec3 motion;
};

struct Triangle {
    uint texture;
    uint surface;
    // The light its model was shaded with, or NO_LIGHT for the world's sun.
    uint light;
    // Its model's heat, for thermal vision.
    float heat;
    // Its material: metalness in the low byte, roughness in the next
    // (255 is one).
    uint material;
    // Its sprite's rows above a share of its height, in 65536ths in the
    // high half (none: 0), take this material instead (materials.sprites).
    uint above;
};

layout(set = 0, binding = 0) uniform accelerationStructureEXT scene;
layout(set = 0, binding = 1, scalar) readonly buffer Vertices { Vertex vertices[]; };
layout(set = 0, binding = 2, scalar) readonly buffer Indices { uint indices[]; };
layout(set = 0, binding = 3, scalar) readonly buffer Triangles { Triangle triangles[]; };
// The variable-size array takes the highest binding.
layout(set = 0, binding = 63) uniform sampler2D textures[];

const uint NO_TEXTURE = 0xffffffffu;
const uint INSTANCE_MODELS = 0u;
const uint INSTANCE_GROUND = 1u;
const uint INSTANCE_WATER = 2u;
const uint INSTANCE_SKY = 3u;
// The models' triangles in masked sprites, whose keyed texels the raster's
// blend shows through: the only triangles a ray tests rather than takes.
const uint INSTANCE_CUTOUT = 4u;
// The additive sprites sunk under the water, which only the water's look
// below it sums (pt_trace.comp's sunk_glow).
const uint INSTANCE_GLOW = 5u;
// The ground's detail patch round the eye, in place of the coarse ground's
// tiles under it (scene_ground_detail.rs).
const uint INSTANCE_DETAIL = 6u;
// The models outside the view and the craft round a first-person eye,
// drawn for the tracer alone (world_scene's outside the view): every ray
// but the eye's own looks for them, so their shadows, reflections and
// bounce light reach the view while it shows only what retail's draw does.
const uint MASK_OUTSIDE = 0x40u;
// Their custom index's bit beside their kind: the radiance cache leaves
// them out (pt_trace.comp's shade_ray).
const uint INSTANCE_OUTSIDE = 0x80u;
// Instance masks: water casts no shadow, so shadow rays and rays already
// under it look for solid things only.
const uint MASK_SOLID = 0x01u | MASK_OUTSIDE;
const uint MASK_GROUND = 0x04u;
const uint MASK_SKY = 0x08u;
// The sunk additive sprites: no ray but the water's looks for them.
const uint MASK_GLOW = 0x10u;
// The models (and their masked sprites) alone, without the ground: the
// local lights test the ground against its smooth surface instead
// (pt_trace.comp's ground_through).
const uint MASK_MODEL = 0x20u | MASK_OUTSIDE;
// Everything but the painted sky: the world.
const uint MASK_WORLD = 0x07u | MASK_OUTSIDE;
// What the eye's own rays see, its view on through the water among them:
// the world, and its solid things, without what lies outside the view.
const uint MASK_VIEW = 0x07u;
const uint MASK_VIEW_SOLID = 0x01u;
// A sky triangle's material bit: its sprite is masked, an opaque object.
const uint SKY_OBJECT_MASKED = 1u << 16;
// A model triangle's material bit: its draw fogs it in a fog of its own
// (retail's actor flash: a hit vehicle's red), which the distance fog
// leaves it.
const uint OWN_FOG = 1u << 17;
// A model triangle's material bit: its surface is bare, no paint under a
// clear coat (the tyres' rubber, materials.sprites), which the rain darkens
// and smooths rather than coats.
const uint BARE = 1u << 18;
// One of the scenery crystals (vk_device/trace.rs's CRYSTAL): only their
// faces take a struck crystal's spark (sparks_at).
const uint CRYSTAL = 1u << 19;

// A candidate hit, which only a masked sprite's triangle gives: solid where
// its texel is (the raster keys the rest away, alpha 0). Those outside the
// view start past the view's, at their instance's record offset.
bool cutout_solid(rayQueryEXT query) {
    int primitive = rayQueryGetIntersectionPrimitiveIndexEXT(query, false) + int(frame.cutout_first)
        + int(rayQueryGetIntersectionInstanceShaderBindingTableRecordOffsetEXT(query, false));
    Triangle triangle = triangles[primitive];
    if (triangle.texture == NO_TEXTURE) {
        return true;
    }
    vec2 bary = rayQueryGetIntersectionBarycentricsEXT(query, false);
    vec2 uv = vertices[indices[primitive * 3 + 0]].uv * (1.0 - bary.x - bary.y)
        + vertices[indices[primitive * 3 + 1]].uv * bary.x
        + vertices[indices[primitive * 3 + 2]].uv * bary.y;
    return textureLod(textures[nonuniformEXT(triangle.texture)], uv, 0.0).a >= 0.5;
}

// Run `query` to its closest hit, through masked sprites' transparent
// texels. A ray that should see them as solid passes gl_RayFlagsOpaqueEXT
// and needs no loop.
#define PROCEED_THROUGH_CUTOUTS(query)                                  \
    while (rayQueryProceedEXT(query)) {                                 \
        if (cutout_solid(query)) {                                      \
            rayQueryConfirmIntersectionEXT(query);                      \
        }                                                               \
    }
