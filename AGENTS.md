# AGENTS.md

Guidance for AI coding agents working in Pastel.

## What this is

Pastel is a scrolling-tiling Wayland compositor (niri-style) written in Rust 2024
on top of [luminaria](https://github.com/antinomie1/luminaria). Luminaria owns
protocols, backends and rendering (native Vulkan 1.3 via vulkan-hpp); Pastel owns policy: where windows go, what a
key does, how things move.

## Commands

```sh
cargo build -j4                 # first build compiles luminaria's C++ (slow, 1-2 GB per unit)
cargo test -j4                  # every crate's tests/; no GPU or parent compositor needed
cargo test -p pastel-layout     # pure crates build no C++: seconds, not minutes
cargo run                       # nested in the current Wayland session
LUMINARIA_BACKEND=headless PASTEL_EXIT_MS=500 cargo run   # smoke test
LUMINARIA_BACKEND=drm cargo run --release                 # bare metal, from a free VT
cargo clippy --all-targets -j4 -- -D warnings
```

Always cap parallelism: `-j4`. Luminaria's module build can exhaust memory otherwise.

## Layout of the code

A cargo workspace. Dependencies only point downwards in this list:

```
crates/
  pastel-core/     Rect, Direction; Curve, Spring, Ease, Anim, AnimRect.
  pastel-layout/   Layout trait, LayoutOp, registry, scrolling columns; Desktop
                   (monitors -> workspaces -> Layout, focus, fullscreen).
  pastel-config/   TOML schema and defaults; Action; key combos and the bind table.
  pastel-render/   Animated window state, Effect pipeline, Overlay trait and the
                   hotkey overlay; turns all of it into luminaria SceneItems.
  pastel-ipc/      IPC wire types (serde, JSON lines), socket path, blocking client.
                   Pure: shells such as Palettes depend on it.
  pastel/          The binary: event loop, clients, input, layer shell, popups, IPC.
```

`pastel-core`, `pastel-layout` and `pastel-config` do not depend on luminaria, so
`cargo test -p <them>` builds no C++ and finishes in seconds. Prefer putting logic
there.

## Rules

- **One source file stays under 1000 lines.** Split before you reach it.
- **Tests live in each crate's `tests/`, never in `src/`.** What a test needs is `pub`.
- **Pure crates stay pure.** Adding luminaria to `pastel-core`, `pastel-layout` or
  `pastel-config` is never the answer. If one seems to need it, the design is wrong.
- **Layouts know nothing about time.** A `Layout` answers "where should windows
  be"; `pastel-render` animates towards the answer. A new layout gets every animation
  for free and must never hold an animation.
- **Time is a parameter.** Anything animated takes `now: f64` (seconds) rather than
  reading a clock, so tests are deterministic.
- **Idle means idle.** With nothing animating the loop blocks in `dispatch(None)`:
  no timers, no polling, no frames. Animations are paced by `Present` events.
  Measure it with a real client after touching the render path: 0 CPU ticks, 0
  context switches.
- **Reuse what luminaria gives you.** `SceneItem`, `PopupTree`, `LayerManager`,
  `ScreenManager`, `InputRouter::route`. Don't re-implement protocol logic.
- **Don't retain `Surface` anything.** Hold `SurfaceId`s and look up on use; stale
  ids resolve to nothing.
- **Don't over-engineer.** A trait exists because something varies now or the
  request called it out (`Layout`, `Curve`, `Effect`, `Overlay`), not "just in case".
- **Comments say why**, in English; user-facing docs may be Chinese.
- **Keep the config concise.** A new option needs a default that is right for most
  people; an empty config file must give a fully working desktop.

## Extending

| To add... | Implement | Register in |
|---|---|---|
| a layout | `pastel_layout::Layout` | `pastel_layout::create` |
| an animation curve | `pastel_core::Curve` | `pastel_config::CurveSpec` |
| a window effect | `pastel_render::Effect` | `pastel_render::Pipeline::from_config` |
| a screen overlay | `pastel_render::Overlay` | `pastel::State` |
| a keybinding action | variant of `pastel_config::Action` | `Action::from_str`, `Action::title` |
| an IPC request or state field | `pastel_ipc::Request` / `pastel_ipc::State` | `State::answer` / `State::snapshot` in `crates/pastel/src/ipc.rs` |

## IPC

`$XDG_RUNTIME_DIR/pastel.$WAYLAND_DISPLAY.sock`, exported to children as
`PASTEL_SOCKET`. One JSON line per message. `Subscribe` streams the whole
`State` whenever it changes; it is published from `State::frame` only on frames
that were dirty, and only to subscribers, so an idle desktop sends nothing.
Changing the wire types changes the protocol for Palettes too: update both.
`crates/pastel/tests/ipc.rs`.

## Luminaria notes

- ext-workspace-v1 (`crates/pastel/src/workspaces.rs`): one group per monitor on its
  `wl_output`, synced from dirty frames with the IPC. Pastel's workspaces are dynamic
  (or a fixed count per monitor with `[layout] workspaces`) and named by position; the protocol can't rename or move one, so a workspace whose
  position or monitor changes is removed and announced again. Only `activate` is
  honoured. `crates/pastel/tests/workspaces.rs`.
- ext-background-effect-v1: layer surfaces (panels, docks) declare blur regions; every
  layer item gets `with_surface_blur` while `window.blur` is on, and luminaria blurs
  only the declared rectangles, nothing for a layer that declared none. `blur-xray`
  applies to the background and bottom layers only: top, overlay and the hotkey
  overlay sit over windows and always blur for real. Windows keep whole-box blur by
  rule.
- Input methods (`crates/pastel/src/ime.rs`): every key Pastel's bindings pass on goes
  through `State::route_key`, which hands it to an input method holding the grab.
  Keys it gives back arrive as `Event::VirtualKey`. Each virtual keyboard has its own
  keymap: the seat switches to it while that keyboard types and back on the next real
  key, as sway does. Candidate windows are scene items under the focused field's text
  cursor. `crates/pastel/tests/input_method.rs`.
- Pointer constraints are luminaria's `InputRouter::follow_constraints`: a locked
  pointer stays put while relative motion flows, a confined one is clamped. Tablets
  go through `luminaria::protocol::TabletFeed`, mapped to the monitor under the pen.
  Gamma and CTM ramps are applied with `Output::set_gamma` / `set_ctm`; each output's
  table size is told to the protocol once it has a `wl_output`, and outputs without a
  table (headless, nested) answer `failed`. `crates/pastel/src/devices.rs`,
  `crates/pastel/tests/devices.rs`.
- Events arrive after the fact in `Display::dispatch`; anything luminaria must
  answer synchronously is told ahead of time (`ScreenManager::set_scene`).
- We do **not** call `InputRouter::attach`. `AutoBackend` forwards all input as
  `Event::Input` regardless, so Pastel matches bindings in Rust and passes the rest
  on with `InputRouter::route`. That is what lets the overlay grab the keyboard and
  Mod+drag steal a click. The cost: `seat.follow`, VT switching and session
  handling are ours (`crates/pastel/src/input.rs`).
- swaybg is a `Background` layer surface per output, named by its `wl_output`.
  It sits above the `background` colour in the scene, so blur samples it; the
  colour only shows where no client paints. `crates/pastel/tests/wallpaper.rs`.
- `SceneKind::Rect` and `Border` honour `corner_radius`; a rounded `Border` stays
  hollow. Never draw anything solid behind a window: it
  shows through translucency and gets sampled into its blur.
- `Output::scale` is an `f64`; `wl_output.scale` is it rounded up. Boxes stay whole
  logical pixels and round to device pixels edge by edge, so a window that doesn't
  fall on a whole device pixel is a pixel off, never a gap. Overlay text is rastered
  at the rounded-up scale and stretched down.
- `Surface` windows are told their scale by `configure_windows`; popups, layer
  surfaces and lock surfaces by `tell_scale` and `tell_output_scales`.
- Touch events don't name their device, so every panel maps to one monitor:
  `touch-output`, a built-in panel, or the focused one, in that order.
- `Toplevel::set_*` state setters each send a configure on change. Call
  `configure(size)` first, so no configure pairs new state with an old size.
- A rounded or translucent window is an offscreen group; luminaria only re-renders
  one when a member commits or moves within it. If frames ever stop being
  `unchanged` at rest, `e2e.rs` should say so.
- `SceneItem::retained` draws a closed window's last pixels, for as long as every
  frame keeps asking. Without a GPU, or for a window never drawn, the close
  animation is a frosted pane instead.
- `crates/pastel/tests/snapshot.rs` reads real pixels by presenting `State::scene`
  to a second headless output and calling `read_back`. Set `PASTEL_SNAPSHOT_DIR` to
  keep the frames.
- luminaria comes from GitHub, pinned by `rev` in the root `Cargo.toml`. To change
  both projects together, add this to that file and don't commit it:
  ```toml
  [patch."https://github.com/antinomie1/luminaria"]
  luminaria = { path = "../luminaria/bindings/rust" }
  ```
