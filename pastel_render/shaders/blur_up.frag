#version 460
#include "vk_gpu.glsl"
#include "blur.glsl"

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 o;

void main() {
    vec2 d = vec2(p.dx, p.dy);
    vec4 s = gpu_sample(p.tex, GPU_LINEAR_CLAMP, uv + vec2(-d.x * 2.0, 0.0));
    s += gpu_sample(p.tex, GPU_LINEAR_CLAMP, uv + vec2(-d.x, d.y)) * 2.0;
    s += gpu_sample(p.tex, GPU_LINEAR_CLAMP, uv + vec2(0.0, d.y * 2.0));
    s += gpu_sample(p.tex, GPU_LINEAR_CLAMP, uv + vec2(d.x, d.y)) * 2.0;
    s += gpu_sample(p.tex, GPU_LINEAR_CLAMP, uv + vec2(d.x * 2.0, 0.0));
    s += gpu_sample(p.tex, GPU_LINEAR_CLAMP, uv + vec2(d.x, -d.y)) * 2.0;
    s += gpu_sample(p.tex, GPU_LINEAR_CLAMP, uv + vec2(0.0, -d.y * 2.0));
    s += gpu_sample(p.tex, GPU_LINEAR_CLAMP, uv + vec2(-d.x, -d.y)) * 2.0;
    o = s / 12.0;
}
