#version 460
#include "vk_gpu.glsl"
#include "blur.glsl"

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 o;

float box_sdf(vec2 x, vec2 h, float r) {
    vec2 d = abs(x) - h + r;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0) - r;
}

void main() {
    vec2 f = gl_FragCoord.xy;
    vec2 center = p.rect.xy + p.rect.zw * 0.5;
    vec2 h = p.rect.zw * 0.5;
    float d = box_sdf(f - center, h, min(p.radius, min(h.x, h.y)));
    float cover = clamp(0.5 - d, 0.0, 1.0) * p.alpha;
    if (cover <= 0.0) {
        discard;
    }
    vec4 bg = gpu_sample(p.tex, GPU_LINEAR_CLAMP, f / p.screen);
    o = vec4(bg.rgb * cover, cover);
}
