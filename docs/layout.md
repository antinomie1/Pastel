# Layout Engine (`pastel_layout`)

## 1. Pluggable `LayoutPolicy` Trait

`pastel_layout` is completely pure and time-free: it never reads a clock or runs animations. Given a workspace's windows and an output's usable area `(x, y, w, h)`, a layout computes target rectangles and configure sizes. Because time is separated into `pastel_render.anim_state`, any custom layout automatically receives spring movement, open/close transitions, and focus-ring animations.

```mica
pub trait LayoutPolicy {
    fn insert(mut self, id: u64, width_hint: f64)
    fn remove(mut self, id: u64) -> bool
    fn focus_dir(mut self, dx: int, dy: int) -> Option[u64]
    fn move_dir(mut self, dx: int, dy: int) -> bool
    fn compute(mut self, area: geom.Rect, gap: f64, mut out: List[TileTarget])
}
```

## 2. Niri-Like Scrolling-Tiling Strip

Each workspace owns a `ScrollingStrip`:

- **Columns**: An ordered `List[Column]`, where each `Column` holds:
  - `width_frac: f64` — width as a fraction of the usable viewport width (clamped to `[0.1, 1.0]`), or maximized (`maximized: bool` -> `1.0`).
  - `tabbed: bool` — when enabled (`toggle-column-tabbed-display`, `Mod+W`), only the active window in the column is emitted with an 8 px top inset for GPU SDF tab pills; inactive tabs are omitted so `luminaria` suspends them automatically.
  - `windows: List[WindowSlot]` — vertically stacked windows inside the column, each with a proportional height weight (default `1.0`).
- **Viewport Camera (`camera_x: f64`)**:
  - Horizontal scroll offset of the strip in logical pixels.
  - Controlled by `center-focus` (`never`, `on-overflow`, `always`) plus `always-center-single-column = true` to center solo columns automatically.
  - Manual scroll/pan (configurable wheel binds, 3-finger horizontal swipe) adjusts `camera_x` directly and snaps to keep the strip cleanly bounded on release.
- **Column Operations**:
  - `consume_or_expel_left` / `consume_or_expel_right` (`Mod+Comma` / `Mod+Period`): Expels from a multi-window column toward that direction, or consumes a solo window into the adjacent column in that direction (`consume_into_column` / `expel_from_column` also retained).
  - `toggle_tabbed` (`Mod+W`): Switches a column between vertical stacking and tabbed display.
  - `cycle_preset_width` (`Mod+R`): Cycles through `[layout].preset-widths` (default `1/3`, `1/2`, `2/3`).
  - `toggle_maximize_column` (`Mod+F`) / `expand_to_available_width` (`Mod+Ctrl+F`): Maximizes to 100% or expands to fill remaining visible viewport width.
  - `center_column` (`Mod+C`): Centers the focused column regardless of `center-focus` (sticky until focus/layout changes).
  - `adjust_row_weight` (`Mod+Shift+Minus` / `Mod+Shift+Equal`) / `reset_row_weights` (`Mod+Ctrl+R`): Adjusts or resets vertical height weights of stacked windows inside a column.

## 3. Workspaces, Overview, Floating Layer & Multi-Monitor

- **Dynamic mode (`workspaces = 0`, default)**: Every monitor starts with one workspace and always keeps one empty spare workspace at the bottom (niri-style). Empty interior workspaces are automatically compacted when switching away.
- **Fixed mode (`workspaces = N`)**: Every monitor allocates `N` persistent workspaces (dwm-tag-style, e.g. `N = 9`). Shrinking `N` on live reload migrates excess windows safely into the last retained workspace.
- **Workspace operations**: `focus-workspace-previous` (`Mod+Tab`), `move-window-to-workspace-silent <1..99>`, and whole-column moves (`move-column-to-workspace*`, `move-column-to-monitor-*`) preserving vertical stacks, row weights, width, and tabbed state.
- **Overview thumbnail layout (`compute_overview`)**: Pure spatial function computing scaled vertical workspace bands (`OverviewBand`) and thumbnail target rects (`TileTarget.overview_scale`) per monitor. `pastel` smoothly interpolates live surfaces via `luminaria.Style.scale` — zero permanent offscreen VRAM, click any window thumbnail or workspace band to jump directly.
- **Floating layer**: Per-workspace list of floating windows stacked globally above tiled columns. Transient dialogs (windows with a `parent` or `modal` flag) and windows matching `floating = true` rules automatically open centered (honoring `default-floating-size` and `open-focused = false`); supports pointer drag/resize and keyboard `move-floating` / `resize-floating`.
- **Fullscreen**: A fullscreen window occupies the full `output_rect(o)` above all layers while hiding occluded windows on that workspace and disabling borders/shadows so direct scanout and on-demand VRR activate cleanly.
- **Multi-monitor**: `Desktop` tracks per-output `Monitor` instances with independent workspace lists, strict per-output tiled clipping (`Layout.window_on`), 2D cross-monitor focus/move for windows and whole columns, and safe orphan staging if all monitors disconnect temporarily.
