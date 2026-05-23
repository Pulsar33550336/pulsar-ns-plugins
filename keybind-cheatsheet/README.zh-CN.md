# 快捷键速查表 — Noctalia

Hyprland 快捷键速查表插件，通过 `hyprctl binds -j` 读取快捷键并支持描述标签分类。

![预览](preview.png)

## 功能

- **仅 Hyprland** — 通过 `hyprctl binds -j` 读取实时快捷键
- **描述标签** — 在 Hyprland 配置中使用 `[分类 N] 描述` 格式注释来组织快捷键
- **隐藏绑定** — 使用 `[hidden]` 标签将快捷键从速查表中排除
- **无描述绑定** — 无描述的绑定也会显示，不会丢弃
- **完整的颜色自定义** — 每种按键类别可独立设置背景和文字颜色，支持实时预览和剪贴板快速粘贴
- **搜索过滤** — 在面板中键入文字以过滤快捷键
- **智能键名格式化** — XF86 键显示为可读名称（如 Vol Up、Bright Down）
- **灵活的列布局**（1-4 列）并支持自动高度
- **IPC 支持** — `toggle` 和 `refresh` 命令支持全局快捷键

## 安装

```bash
cp -r keybind-cheatsheet ~/.config/noctalia/plugins/
```

## 使用方法

### 任务栏组件
在 Noctalia 设置中将插件添加到任务栏。点击键盘图标打开速查表。

### 全局快捷键

**Hyprland：**
```bash
bind = $mod, F1, exec, qs -c noctalia-shell ipc call plugin:keybind-cheatsheet toggle
```

### IPC 命令

| 命令 | 效果 |
|------|------|
| `qs -c noctalia-shell ipc call plugin:keybind-cheatsheet toggle` | 打开/关闭速查表面板 |
| `qs -c noctalia-shell ipc call plugin:keybind-cheatsheet refresh` | 强制重新解析快捷键 |

修改配置后可使用 `refresh` 重新加载，无需重启 shell。

## 描述标签

在 Hyprland 中使用 `bindd`，将描述作为第三个参数，格式为 `[分类 N] 描述`：

```bash
bindd = SUPER, Super_L, [启动器/Shell 1] 启动器, exec, $ipc launcher toggle
bindd = Super, V, [启动器/Shell hidden] 剪贴板历史 >> 剪贴板, exec, $ipc launcher clipboard
bindd = Super, J, [启动器/Shell 8] 切换顶栏, exec, $ipc bar toggle
```

- `分类` — 速查表中显示的分组名称
- `N` — 可选的优先级数字，用于排序
- `描述` — 显示在快捷键旁边的文字
- `[hidden]` — 从速查表中隐藏该绑定

## 特殊键名格式

| 原始键名 | 显示 |
|---------|------|
| `XF86AudioRaiseVolume` | Vol Up |
| `XF86AudioLowerVolume` | Vol Down |
| `XF86AudioMute` | Mute |
| `XF86MonBrightnessUp` | Bright Up |
| `XF86MonBrightnessDown` | Bright Down |
| `Print` | PrtSc |
| `Prior` / `Next` | PgUp / PgDn |

## 颜色自定义

每种按键类别可独立设置**背景**和**文字**颜色：
`Super`、`Ctrl`、`Shift`、`Alt`、`XF86`、`Print`、数字键、鼠标键和默认字母键 — 以及描述文字颜色。

- **双胶囊行**：左胶囊 = 背景色，右胶囊 = 该背景上的标签文字色。点击胶囊打开颜色选择器。
- **主题感知默认值**：`Super` / `Ctrl` / `Shift` 使用空值标记"使用 Material 主题强调色"（`mPrimary` / `mSecondary` / `mTertiary`），因此主题设置保持不受影响，除非你手动覆盖。
- **剪贴板快速粘贴**：复制 `#RRGGBB` / `#RRGGBBAA` 十六进制颜色后，每个胶囊内会出现粘贴图标 — 点击即可应用（剪贴板通过 `wl-paste` 轮询）。
- **实时预览 + 还原**：更改立即预览；关闭设置而不点击应用将恢复打开时保存的快照。
- **单行重置**和**"重置所有颜色"**操作可恢复主题默认值。

## 设置

通过面板标题栏的齿轮图标进入设置：

- **窗口宽度/高度**（自动或手动）和**列数**（1-4）
- **显示无描述的绑定**开关
- **按键颜色** — 每种类别的背景和文字颜色，支持剪贴板粘贴和重置
- **符号显示** — macOS 风格、功能键和鼠标 Nerd Font 符号开关
- **Super 键文字** — 自定义文字或 Nerd Font 码位
- **刷新** — 强制重新加载快捷键

## 系统要求

- Noctalia Shell 3.6.0+
- Hyprland
- `hyprctl` 在 `PATH` 中
- `wl-paste`（wl-clipboard）用于颜色剪贴板快速粘贴功能

## 许可证

MIT
