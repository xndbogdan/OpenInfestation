#version 450
// A quad over the viewport from the vertex index (a four-vertex strip).
// Texture row 0 lands on the viewport's top row.
layout(location = 0) out vec2 v_texture;
void main() {
    vec2 corner = vec2(gl_VertexIndex & 1, gl_VertexIndex >> 1);
    v_texture = corner;
    gl_Position = vec4(corner * 2.0 - 1.0, 0.0, 1.0);
}
