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
  - `windows: List[WindowSlot]` — vertically stacked windows inside the column, each with a proportional height weight (default `1.0`).
- **Viewport Camera (`camera_x: f64`)**:
  - Horizontal scroll offset of the strip in logical pixels.
  - Controlled by `center-focus`:
    - `never`: Minimal scroll so the focused column stays completely inside `[camera_x, camera_x + area.w]`.
    - `on-overflow`: Scroll minimally, or center the focused column if it was off-screen.
    - `always`: Always center the focused column in the viewport.
  - Manual scroll/pan (`Mod+Scroll`, 3-finger horizontal swipe) adjusts `camera_x` directly and snaps to keep the strip cleanly bounded on release.
- **Column Operations**:
  - `consume_into_column` (`Mod+[`): Pulls the focused window into the left adjacent column (or absorbs the right neighbor if already in column 0).
  - `expel_from_column` (`Mod+]`): Moves the focused window out of a multi-window column into its own column to the right.
  - `cycle_preset_width` (`Mod+R`): Cycles through `[layout].preset-widths` (default `1/3`, `1/2`, `2/3`).
  - `toggle_maximize_column` (`Mod+F`): Toggles full viewport width while remembering the prior fraction.
  - `center_column` (`Mod+C`): Centers the focused column regardless of `center-focus` (sticky until focus/layout changes).
  - `adjust_row_weight` (`Mod+Shift+Minus` / `Mod+Shift+Equal`): Adjusts vertical height weights of stacked windows inside a column.

## 3. Workspaces, Floating Layer & Multi-Monitor

- **Dynamic mode (`workspaces = 0`, default)**: Every monitor starts with one workspace and always keeps one empty spare workspace at the bottom (niri-style). Empty interior workspaces are automatically compacted when switching away.
- **Fixed mode (`workspaces = N`)**: Every monitor allocates `N` persistent workspaces (dwm-tag-style, e.g. `N = 9`). Shrinking `N` on live reload migrates excess windows safely into the last retained workspace.
- **Floating layer**: Per-workspace list of floating windows stacked globally above tiled columns. Transient dialogs (windows with a `parent` or `modal` flag) and windows matching `floating = true` rules automatically open centered in the floating layer; supports pointer drag/resize and keyboard `move-floating` / `resize-floating`.
- **Fullscreen**: A fullscreen window occupies the full `output_rect(o)` above all layers while hiding occluded windows on that workspace and disabling borders/shadows so direct scanout activates cleanly.
- **Multi-monitor**: `Desktop` tracks per-output `Monitor` instances with independent workspace lists, strict per-output tiled clipping (`Layout.window_on`), 2D cross-monitor focus/move (`focus-monitor-left/right/up/down`, `move-window-to-monitor-*`), and safe orphan staging if all monitors disconnect temporarily.
