# 配置

## SPlayer-Next 插件

| 设置 | 默认 | 作用 |
| --- | --- | --- |
| 桥地址 | `127.0.0.1` | `bridge.py` 的监听地址 |
| 桥端口 | `33550` | 必须与 Noctalia 插件、桥一致 |
| 过滤制作人员行 | 开 | 跳过作词/作曲/编曲/演奏/制作等制作信息行 |
| 制作人员关键词 | 内置列表 | 逗号、分号、换行或 `|` 分隔；按字面匹配 |
| 过滤歌手/专辑署名 | 开 | 短行包含歌手/专辑名时按制作信息处理；有翻译的歌词行不参与此过滤 |
| 心跳间隔（秒） | 5 | 定时重发状态，让桥知道播放器仍在运行 |

## Noctalia 组件

| 设置 | 默认 | 作用 |
| --- | --- | --- |
| 最大字符数 | 40 | 超出后截断并加省略号；0 表示不限制 |
| 轮询间隔 | 300ms | 向桥询问当前行 |
| 副行模式 | `duet-translation-next` | 回退路径，取值见下 |
| 文字前缀 | 空 | 显示在歌词前的固定文字 |
| 空闲时隐藏 | 开 | 桥离线、未播放或无歌词时隐藏组件 |
| 暂停时保留歌词 | 关 | 暂停时保留最后一行 |
| 无歌词时显示歌名 | 开 | 纯音乐段落或无歌词时显示歌名 |

副行模式（回退路径，按顺序取第一个可用的候选）：

- `duet-translation-next`：对唱 → 翻译 → 下一行；
- `duet-translation`：对唱 → 翻译；
- `translation-next`：翻译 → 下一行；
- `duet-next`：对唱 → 下一行；
- `duet`：只显示对唱；
- `translation`：只显示翻译；
- `next`：只显示下一行；
- `none`：无副行。

背景和声行（`isBG`）不进入副行候选；当前行是背景和声时，主行回退到它所属的主行，和声行本身不会出现在副行。

组件只负责显示，不处理点击或悬停；左键可以留给用户自己的绑定，中键是 Noctalia 内建的组件设置入口。

## 状态栏配置

组件类型为 `pulsar/splayer-lyrics:widget`。可以直接写配置：

```toml
[widget.splayer_lyrics]
type = "pulsar/splayer-lyrics:widget"

[bar.default]
end = [
    "tray",
    "volume",
    "brightness",
    "splayer_lyrics",
    "clock",
    "notifications",
    "control-center",
]
```

如果 `[bar.default].end` 已存在，只需把 `"splayer_lyrics"` 插到想要的位置，不要覆盖整段。Noctalia 的 GUI 设置写在 `~/.local/state/noctalia/settings.toml`，优先级高于 `~/.config/noctalia/config.toml`。

## 制作人员关键词

默认列表覆盖常见中文角色（作词、作曲、编曲、制作人、混音、母带、录音、和声、演唱、吉他、贝斯、鼓、钢琴、弦乐、乐队、乐团、萨克斯、小号、长号、统筹、封面、设计、翻译、校对等）和英文角色（Music、Lyrics、Composer、Arranger、Producer、Mixing、Mastering、Guitar、Bass、Drums、Piano、Violin、Cello、Orchestra、Sax、Trumpet、Vocals、Recorded、Studio、Engineer、Written、Composed、Arranged、Produced、Performed、Lyricist、Songwriter 等）。

匹配规则：

1. 行首是关键词 + `:` / `：`，例如 `作词 : 张三`、`Composer: Someone`；
2. 行内任意位置出现关键词 + `:` / `：`，例如 `萨克斯 Sax：李世海`；
3. 行首是较长中文关键词 + 空格 + 内容，例如 `作词 张三`；
4. 英文角色允许关键词和冒号之间夹修饰词，如 `Music Copyist：`、`Produced by：`。

单字符关键词（`词`、`曲`、`鼓`、`笛` 等）只认冒号形式，避免误伤「一曲」「曲终人散」。关键词列表留空则只保留歌手/专辑署名过滤；完全关闭请用「过滤制作人员行」总开关。

制作署名拆成两行时（第二行没有冒号，例如 `prime sound studio form`），如果它夹在两条制作行之间，也会按制作信息处理。当前行是制作信息时，组件回退到上一条正常歌词并保留它的翻译；制作署名不会作为副行。

## 改端口

三处必须一致：

1. Noctalia 插件设置里的 **Bridge port**；
2. SPlayer-Next 插件设置里的 **桥端口**；
3. 手动运行 `bridge.py` 时的 `--port`（如果用过）。

改完插件重载后，`bridge` service 会停掉旧桥，在新端口拉起。
