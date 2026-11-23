// Shared push constants for Dual-Kawase blur and SDF frosted-glass composite.
layout(push_constant) uniform BlurPush {
    uint tex;
    float dx;
    float dy;
    float alpha;
    vec4 rect;
    vec2 screen;
    float radius;
    float pad;
} p;
