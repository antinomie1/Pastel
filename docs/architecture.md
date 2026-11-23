# Architecture & WDDM/DWM Model

## 1. Design Philosophy

Pastel combines two proven systems-engineering ideas on top of Mica and `luminaria`:

1. **WDDM + DWM separation of concerns**:
   - **Device & Protocol Layer (`luminaria`)** acts as the WDDM display driver/kernel boundary: Wayland protocol state machines (including `wp-color-management-v1`, `wlr-gamma-control-unstable-v1`, `wlr-virtual-pointer-unstable-v1`, `ext-session-lock-v1`, `ext-image-copy-capture-v1` with per-window redaction), DRM/KMS atomic commit thread with real-time scheduling, explicit sync (`sync_file` / `drm_syncobj`), HDR static metadata & CRTC gamma LUT, direct scanout, Multi-Plane Overlays (MPO), multi-seat input (`libseat` / `libinput` / `xkbcommon`), adaptive vblank clock, and GPU device-lost (TDR) recovery.
   - **Visual Composition Layer (`pastel_render`)** acts as the DWM visual compositor: manages the visual effect sheet (hollow SDF borders, Dual-Kawase frosted-glass blur, tab indicator pills, overview workspace frames, shadows, dynamic background, hotkey overlay) and lazy multi-format HDR/SDR canvases while preserving zero-copy direct scanout and MPO promotion whenever possible.
   - **Windowing Policy Layer (`pastel_layout` + `pastel`)** decides where windows belong, how workspaces/overview/tabbed columns/focus behave, lock-screen shortcut isolation, on-demand VRR policy, and how user input translates into actions.

2. **Pluggable, monomorphized modularity**:
   - Mica traits are structural and monomorphized at compile time (`Display[O, R]`, `Pastel[O, R]`, `Curve`, `LayoutPolicy`), giving clean plug-in boundaries with zero virtual-call indirection.
   - Dependencies flow strictly downward:

```
pastel (binary: event loop, input, windows, outputs, reload, spawn)
  ├── pastel_ipc    (Unix JSON-lines IPC)
  ├── pastel_render (PastelRenderer, Dual-Kawase blur, SDF borders, hotkey overlay, anim state)
  ├── pastel_layout (LayoutPolicy, scrolling strip, workspaces, multi-monitor)
  ├── pastel_config (TOML parser, actions, keybindings, window rules)
  └── pastel_core   (geometry, color, Curve, Spring/Ease, Anim)
        └── luminaria / vk_gpu / vk_draw2d / vk_compose
```

## 2. Data Structures & Performance Choices

| Component | Structure | Rationale |
|---|---|---|
| Columns & windows in strip | Contiguous `List[Column]` / `List[WindowSlot]` | Typical N = 1..50; cache-line locality beats trees or linked lists by an order of magnitude |
| Window lookup (`WindowId -> location`) | Open-addressed `Map[u64, WindowLoc]` | O(1) lookup on commit, close, resize, and focus without scanning workspaces |
| Per-frame placement & decoration lists | Reusable scratch buffers via `mem.replace` | Zero heap allocation per frame in steady state |
| Animation springs | Analytical ODE state (`pos`, `vel`, `target`, `t0`) | O(1) evaluation at exact `vblank_ns`; refresh-rate independent; zero drift |
| Blur textures | Lazily allocated half/quarter-res ping-pong pyramid | Zero VRAM when blur is unused or windows are opaque; cached in X-Ray mode |
| Overlay glyph rendering | Dynamically loaded FreeType2 + Fontconfig (`extern table`) | Zero extra link-time dependencies; crisp anti-aliased TrueType/OpenType text matching `[overlay].font` |

## 3. Event Loop & Zero-Idle Guarantee

Pastel runs a single-threaded epoll event loop owned by `luminaria.Display`:

1. `d.next()` returns queued events immediately; when the queue empties, `luminaria` finishes due frames, flushes all clients in a single `sendmsg` per client, and sleeps in `epoll_wait(-1)`.
2. Pastel watches its own IPC listener and client sockets via `d.watch(fd, token)` — no background polling threads.
3. While any animation is active, Pastel evaluates springs/eases at `Event.Frame(o, vblank_ns)` and signals `luminaria`'s adaptive vblank clock via a culled 1x1 offscreen tick sentinel. The moment all animations settle, the sentinel is omitted and the compositor sleeps with **0 wakeups/second** and **0.0% CPU**.
