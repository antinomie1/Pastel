#version 460
#include "blur.glsl"

layout(location = 0) out vec2 uv;

void main() {
    vec2 pos = vec2((gl_VertexIndex == 1) ? 3.0 : -1.0, (gl_VertexIndex == 2) ? 3.0 : -1.0);
    uv = pos * 0.5 + 0.5;
    gl_Position = vec4(pos, 0.0, 1.0);
}
