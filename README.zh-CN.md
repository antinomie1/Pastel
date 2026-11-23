# Pastel

[English](README.md) | 简体中文

基于 [Mica](https://github.com/antinomie1/mica) 语言与 [luminaria](https://github.com/antinomie1/luminaria) 构建的轻量级、高性能滚动平铺（niri 风格）Wayland 窗口管理器与合成器。

## 核心特性

- **滚动平铺（Scrolling Tiling）**：每个工作区拥有无限水平滚动的列带（column strip），支持列内垂直堆叠、预设宽度循环、列最大化与居中、动态（niri 风格）或固定（dwm tag 风格）工作区、浮动窗口与多显示器支持。
- **解析式动画系统**：采用精确解析 ODE 弹簧求解器与缓动曲线，与刷新率无关（60Hz / 144Hz / 240Hz / VRR），支持运动中保动量重定位（in-flight retargeting），覆盖窗口移动、视口滚动、工作区切换、窗口开关、焦点环颜色过渡与热键层动画。
- **视觉效果**：默认方角窗口边框（`corner-radius = 0.0`，可按需调整圆角）、GPU SDF 空心边框、柔和阴影、窗口透明度与 GPU Dual-Kawase 毛玻璃背景模糊（支持稳态零额外 GPU Pass 的 X-Ray 模糊与实时背景模糊）。
- **视觉一致的热键表**：简洁的 niri 风格热键覆盖层（支持启动显示与 `Mod+Shift+/` 切换），运行时按需动态加载系统 FreeType2 / Fontconfig 渲染清晰平滑的矢量字体（默认 `[overlay].font = "sans-serif"`），外框宽度、方角/圆角、主题色、阴影、实时模糊与动画曲线自动继承 `[window]` 配置。
- **模块化与可插拔**：基于 Mica 结构化 trait 单态化（`Curve`、`LayoutPolicy`、`luminaria.plug.Output`、`luminaria.plug.Renderer`），无虚表与动态分发开销。
- **低占用与边缘设备友好**：零额外字体/UI C 库依赖、模糊纹理按需惰性分配、热路径稳态零堆分配、**空闲时零唤醒**。
- **TOML 配置与热重载**：严格配置校验（`pastel --check`），空配置文件即可直接获得完整桌面，支持运行时无重启热重载。
- **JSON-lines IPC**：通过 Unix Socket 提供 `pastel msg <action>`、`pastel msg --state`、`pastel msg --subscribe`，便于与状态栏、启动器及脚本集成。

## 构建

需要 `mica` 编译器、`sheaf` 包管理器、Vulkan 1.3 运行时，以及 `luminaria` 所需的系统库（`libdrm`、`libinput`、`libseat`、`libudev`、`xkbcommon`）。

```sh
sheaf build --release -p pastel
```

运行全部单元测试与性能基准测试：

```sh
sheaf test
sheaf test --release -p pastel
```

## 运行

```sh
# 在现有 Wayland 会话中嵌套运行（Mod = Alt）
./target/release/pastel

# 在空闲 VT 上裸机 DRM/KMS 运行（Mod = Super）
./target/release/pastel --backend drm

# 仅校验配置文件
./target/release/pastel --check

# 列出所有可绑定的动作
./target/release/pastel --list-actions
```

### 默认快捷键

默认终端为 `foot`，默认启动器为 `fuzzel`（可在 `[binds]` 中覆盖）。

| 快捷键 | 动作 |
|---|---|
| `Mod+Shift+/` | 显示 / 隐藏热键表 |
| `Mod+Return` / `Mod+D` | 启动终端 / 应用启动器 |
| `Mod+Q` | 关闭当前焦点窗口 |
| `Mod+H` / `Mod+L` | 聚焦左 / 右列 |
| `Mod+J` / `Mod+K` | 列内聚焦下 / 上窗口 |
| `Mod+Shift+H` / `Mod+Shift+L` | 向左 / 右移动列 |
| `Mod+Shift+J` / `Mod+Shift+K` | 向下 / 上移动窗口 |
| `Mod+[` / `Mod+]` | 将窗口并入相邻列 / 移出为新列 |
| `Mod+R` / `Mod+F` / `Mod+C` | 循环列宽 / 最大化列 / 居中列 |
| `Mod+Shift+F` | 切换全屏 |
| `Mod+V` / `Mod+Shift+V` | 切换浮动 / 在平铺与浮动层间切换焦点 |
| `Mod+U` / `Mod+I` | 切换上一 / 下一工作区 |
| `Mod+1` .. `Mod+9` | 切换到工作区 1..9 |
| `Mod+Shift+1` .. `Mod+Shift+9` | 将窗口移至工作区 1..9 |
| `Mod+Shift+R` | 热重载配置 |
| `Mod+Shift+E` | 退出 Pastel |
| `Mod+滚轮` | 水平滚动列带 |
| `Mod+左键拖动` / `Mod+右键拖动` | 移动 / 缩放浮动窗口 |
| 触控板三指滑动 | 水平平移列带 / 垂直切换工作区 |

## 配置

默认配置文件路径为 `~/.config/pastel/config.toml`（也可通过 `--config <path>` 指定）。空文件即提供完整默认桌面，全部选项见 [config.example.toml](config.example.toml)。

```toml
background = "#1e2028"

[layout]
gap = 12.0
center-focus = "on-overflow"

[window]
corner-radius = 0.0        # 默认方角边框
border-width = 2.0
active-color = "#a6c8ff"
inactive-color = "#4a4f5c"
blur = true
blur-xray = true

[animation]
slowdown = 1.0

[binds]
"Mod+B" = "spawn firefox"
"Mod+D" = "none"           # 移除默认绑定

[[window-rule]]
app-id = "foot"
opacity = 0.88
blur = true
```

## IPC

```sh
pastel msg focus-column-right
pastel msg focus-workspace 2
pastel msg reload-config
pastel msg --state
pastel msg --subscribe
```

## 架构与文档

| 包 | 职责 | 可插拔扩展点 |
|---|---|---|
| `pastel_core` | 几何、颜色、解析式弹簧/缓动动画 | `Curve` |
| `pastel_config` | 严格 TOML 配置、动作表、键位绑定、窗口规则 | `Action` |
| `pastel_layout` | 滚动平铺列带、工作区、浮动层、多显示器 | `LayoutPolicy` |
| `pastel_render` | Vulkan 渲染器（Dual-Kawase 模糊、SDF 边框）、热键覆盖层 | `luminaria.plug.Renderer` |
| `pastel_ipc` | Unix JSON-lines IPC 服务端与命令行客户端 | `Request` / `Event` |
| `pastel` | 事件循环、输入手势、窗口规则、热重载、进程启动 | `Pastel[O, R]` |

详细架构与设计文档见 [docs/README.md](docs/README.md)，开发规范见 [AGENTS.md](AGENTS.md)。

## 许可证

采用 BSD 2-Clause 许可证，详见 [LICENSE](LICENSE)。
