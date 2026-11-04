# Pastel

[English](README.md) | 简体中文

一个滚动平铺的 Wayland 窗口管理器，基于 [luminaria](https://github.com/antinomie1/luminaria)，用 Rust 2024 编写。

- **原生 Vulkan 1.3 后端**：使用 luminaria 的原生 Vulkan 1.3 后端
- **滚动平铺**：列、列内堆叠、预设宽度、最大化、居中、动态或固定数量的工作区（`workspaces = 9`，如 dwm 的标签）、多显示器
- **动画**：弹簧与缓动两种曲线，可在飞行中改目标并保留速度；窗口开关、移动、工作区切换、焦点色、overlay 全部覆盖
- **视觉效果**：可选圆角（默认关闭）、空心边框、阴影、磨砂模糊、窗口规则
- **浮动窗口**：对话框自动浮动，也可手动切换或用窗口规则指定；标题栏拖动、`Mod+左键` 移动、`Mod+右键` 缩放
- **桌面协议**：锁屏（swaylock）、截屏（grim、wf-recorder）、空闲通知（swayidle）、任务栏与面板（经 ext-background-effect-v1 模糊其后背景）、分页器工作区（ext-workspace-v1）、任务栏与窗口列表（wlr-foreign-toplevel、ext-foreign-toplevel-list）、输出管理（wlr-randr、kanshi）、剪贴板管理器（wlr-data-control）、窗口图标（xdg-toplevel-icon）、色彩管理协商（sRGB）、xdg-activation
- **输入法**：经 text-input-v3 与 input-method-v2 支持 fcitx5、IBus，候选窗显示在光标下方；自带 keymap 的虚拟键盘（wtype）与虚拟指针（wayvnc、KDE Connect）
- **游戏与绘画**：指针锁定、限制与相对运动；数位板（笔、橡皮、pad）
- **夜间模式**：wlsunset、gammastep、hyprsunset 经 wlr-gamma-control 与 hyprland-ctm-control 作用于 DRM 输出
- **X11 应用**：通过 [xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite)，第一个 X 客户端连接时才启动，`[xwayland] enabled` 可关闭
- **显示**：小数缩放（`scale = 1.5`）、旋转、多屏，客户端按真实缩放绘制
- **输入**：触摸屏（随屏幕旋转，`touch-output` 指定所在显示器）；触控板三指左右平移、上下切工作区，其余手势交给应用
- **快捷键菜单**：启动时显示，`Mod+Shift+/` 再次打开，任意键关闭
- **配置热重载**：保存配置文件即生效；键盘布局、背景色、显示器（`[[output]]`）都不用重启
- **IPC**：`pastel msg focus-workspace 2`、`pastel msg --state`、`pastel msg --subscribe`；Unix socket 上的 JSON 行协议（`pastel-ipc` crate），供状态栏与 [Palettes](https://github.com/antinomie1/Palettes) 等 shell 使用
- **静止时零开销**：没有动画就没有定时器、没有帧

## 构建

需要 Rust ≥ 1.88、xmake、clang ≥ 22，以及 wayland、wayland-protocols、vulkan-headers、libinput、seatd、libdrm、gbm、fcft、glslang。

luminaria 由 cargo 从 GitHub 拉取，版本固定在根 `Cargo.toml` 的 `rev`：

```sh
cargo build --release -j4      # 首次要编译 C++，请限制并行度
```

## 运行

```sh
cargo run --release            # 在当前 Wayland 会话里以窗口方式运行（Mod = Alt）
LUMINARIA_BACKEND=drm cargo run --release   # 在空闲 VT 上裸机运行（Mod = Super）
```

默认终端是 `foot`，启动器是 `fuzzel`。

| 按键 | 动作 |
|---|---|
| `Mod+Shift+/` | 快捷键菜单 |
| `Mod+Return` / `Mod+D` | 终端 / 启动器 |
| `Mod+Q` | 关闭窗口 |
| `Mod+H/L`、`Mod+J/K` | 切换列、列内切换焦点 |
| `Mod+Shift+H/J/K/L` | 移动 |
| `Mod+[` / `Mod+]` | 并入相邻列 / 独立成列 |
| `Mod+R`、`Mod+F`、`Mod+C` | 循环宽度、最大化、居中 |
| `Mod+Shift+F` | 全屏 |
| `Mod+V` / `Mod+Shift+V` | 切换浮动 / 在浮动与平铺间切换焦点 |
| `Mod+U/I`、`Mod+1..9` | 切换工作区 |
| `Mod+Shift+R` / `Mod+Shift+E` | 重载配置 / 退出 |
| `Mod+滚轮`、`Mod+左键拖动` | 切换列、平移视图（在浮动窗口上是移动它） |
| `Mod+右键拖动` | 缩放窗口 |

## 配置

`~/.config/pastel/config.toml`。空文件就是完整可用的桌面，只写想改的项；所有选项见 [config.example.toml](config.example.toml)。

```toml
[layout]
gap = 8
center-focus = "on-overflow"

[animation]
slowdown = 1.5

[binds]
"Mod+B" = "spawn firefox"
"Mod+D" = "none"             # 取消默认绑定

[[window-rule]]              # 模糊要透过半透明窗口才看得到
app-id = "foot"
opacity = 0.85
```

`pastel --check` 校验配置，`pastel --list-actions` 列出所有动作。

## 扩展

代码按 crate 分层，依赖只向下：

| crate | 职责 | 扩展点 |
|---|---|---|
| `pastel-core` | 几何、动画 | `Curve` |
| `pastel-layout` | 布局、桌面模型 | `Layout` |
| `pastel-config` | 配置、动作、按键 | `Action` |
| `pastel-render` | 动画状态、效果、overlay | `Effect`、`Overlay` |
| `pastel-ipc` | IPC 协议、socket 路径、客户端 | `Request` |
| `pastel` | 事件循环、客户端、输入、IPC | — |

布局只回答“窗口应该在哪”，不接触时间，所以新布局自动获得全部动画。开发约定见 [AGENTS.md](AGENTS.md)，待办见 [TODO.md](TODO.md)。

## 许可证

BSD 2-Clause，见 [LICENSE](LICENSE)。
