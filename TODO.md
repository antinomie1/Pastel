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

## Desktop protocols

- [x] Pointer constraints and relative pointer (games)
- [x] Virtual keyboards with their own keymap (wtype)
- [x] Tablets: pens, erasers and pads, mapped to the monitor under the pen
- [x] Gamma and CTM (night light) on DRM outputs
- [ ] Constraints under the nested backend: it only reports absolute positions, so
      nothing is locked or confined there and no relative motion exists
- [ ] Tablets mapped to a chosen output (`[[tablet]] output = ...`) rather than the one
      under the pen

## Needs real hardware

Headless can't reach these; they pass everything short of them.

- [ ] Fractional scale, and a captured cursor, on a real KMS plane
- [ ] `touch-output` with two monitors
