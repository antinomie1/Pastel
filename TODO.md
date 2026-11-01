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

## Later

- [ ] Floating windows; dialogs currently become columns
- [ ] Interactive move and resize; `RequestMove`/`RequestResize` are ignored
- [ ] Touchpad gestures
- [ ] Session lock, idle notify, foreign toplevel, screencopy, output management
- [ ] XWayland
- [ ] Per-output config (scale, mode, position); fractional scale is primary-only
- [ ] Layer surfaces always land on the focused monitor: `is_on` needs an `OutputGlobal`
- [ ] Keyboard layout config; `input` has none yet
- [ ] Touch input
- [ ] xdg-activation, so launchers and links can raise windows
- [ ] `background` needs a restart; ScreenManager takes it once
- [ ] Clip a rounded border properly at a shared monitor edge
- [ ] Config hot reload via inotify (Mod+Shift+R works today)
- [ ] Close animation from real pixels; needs luminaria to retain a texture
- [ ] Sharp overlay text on HiDPI; luminaria images are 1:1 with their logical box
- [ ] Predict the animation clock from `PresentEvent` instead of `Instant::now()`
