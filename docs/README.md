# Pastel Design Documentation

- [Architecture & WDDM/DWM Model](architecture.md) — Crate layering, WDDM/DWM separation of concerns, data structures, zero-idle event loop, and performance model.
- [Layout Engine](layout.md) — Pluggable `LayoutPolicy` trait, niri-like scrolling-tiling column strip, viewport camera, workspaces, floating layer, and multi-monitor routing.
- [Animation System](animation.md) — Structural `Curve` trait, exact analytical ODE spring solver, easing curves, momentum-preserving retargeting, and vblank synchronization.
- [Rendering, Blur & Hotkey Overlay](render.md) — `PastelRenderer` (`luminaria.plug.Renderer`), Dual-Kawase backdrop blur (X-Ray & Live), GPU SDF hollow borders, hardware fast paths, and visually consistent zero-dep hotkey overlay.
- [Configuration & IPC](config-and-ipc.md) — Strict TOML schema, keybinding engine, window rules, live reload, and Unix JSON-lines IPC.
