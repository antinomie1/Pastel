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

- [ ] Fractional output scale; `Output::scale` is an integer in luminaria
- [ ] Touch on a rotated or second screen: points map linearly onto the focused monitor
- [ ] Window capture of a tiled dmabuf; `ScreencopyManager::set_renderer` wants the
      `VulkanRenderer` inside `SceneRenderer`, which the binding doesn't expose
- [ ] A cursor on a hardware plane is missing from captures: `set_cursor_source`
- [ ] An idle inhibitor on a subsurface counts as hidden; only root surfaces are in the scene
