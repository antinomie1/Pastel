# TODO

## Milestones

- [x] Repo, AGENTS.md, TODO.md, build against luminaria
- [x] `pastel-core`: curves, retargetable animations
- [x] `pastel-layout`: `Layout` trait, scrolling columns, desktop model
- [x] `pastel-config`: TOML, actions, bind table
- [x] `pastel-render`: animated window state, effects (border, shadow, blur, opacity)
- [x] `pastel`: event loop, windows, layer shell, popups, pointer, decorations
- [x] Hotkey overlay
- [x] End-to-end test with a real client on the headless backend
- [x] README and example config
- [x] IPC socket (`pastel-ipc`, `pastel msg`) for shells and bars

## Later

- [x] Fractional output scale
- [x] Touch on a rotated or second screen
- [x] Window capture of a tiled dmabuf
- [x] A cursor on a hardware plane in captures
- [x] An idle inhibitor on a subsurface
- [ ] Two touchscreens at once: touch events don't name their device, so both land
      on one monitor
- [ ] A popup is told the focused monitor's scale, not its parent's; only mixed-scale
      monitors can tell
- [ ] A rounded or translucent window at a fractional scale is an offscreen group
      whose size can differ from its place on screen by a pixel, and is resampled

## Waiting on luminaria

luminaria implements these; its Rust binding or backend can't carry them yet.

- [ ] Pointer constraints and relative pointer (games): the binding sends no relative
      motion, and a locked pointer needs it
- [ ] Virtual keyboards with their own keymap (wtype): the keymap isn't exposed
- [ ] Tablets: the binding leaves mapping each event to the caller
- [ ] Gamma and CTM (night light): the backend can't apply a ramp or matrix

## Needs real hardware

Headless can't reach these; they pass everything short of them.

- [ ] Fractional scale, and a captured cursor, on a real KMS plane
- [ ] `touch-output` with two monitors
