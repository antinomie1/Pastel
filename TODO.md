# TODO

## Milestones

- [ ] Repo, AGENTS.md, TODO.md, build against luminaria
- [ ] `pastel-core`: curves, retargetable animations
- [ ] `pastel-layout`: `Layout` trait, scrolling columns, desktop model
- [ ] `pastel-config`: TOML, actions, bind table
- [ ] `pastel-render`: animated window state, effects (border, shadow, blur, opacity)
- [ ] `pastel`: event loop, windows, layer shell, popups, pointer, decorations
- [ ] Hotkey overlay
- [ ] End-to-end test with a real client on the headless backend
- [ ] README and example config

## Later

- [ ] Floating windows; dialogs currently become columns
- [ ] Interactive resize; touchpad gesture scrolling
- [ ] Session lock, idle notify, foreign toplevel, screencopy, output management
- [ ] XWayland
- [ ] Per-output config (scale, mode, position)
- [ ] Config hot reload via inotify (Mod+Shift+R works today)
- [ ] Close animation from real pixels; needs luminaria to retain a texture
- [ ] Sharp overlay text on HiDPI; luminaria images are 1:1 with their logical box
- [ ] Predict the animation clock from `PresentEvent` instead of `Instant::now()`
