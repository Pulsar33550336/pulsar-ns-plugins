# 设计说明

## 组件

- **Noctalia widget**：轮询 `bridge.py` 的 `/state`，显示当前歌词。
- **bridge service**：随 Noctalia 插件启用而拉起 `bridge.py`，负责停止与重启。
- **SPlayer-Next 插件**：订阅播放事件，把当前行 POST 给桥。

## 为什么需要桥

SPlayer-Next 的插件运行在 Electron utility process 里，可以发 HTTP 请求，但不能监听端口；Noctalia 的 Luau 插件只能发请求，也不能监听。两边无法直接连接，因此中间需要一个能监听端口的本地进程：

```text
SPlayer-Next 插件 --POST /lyric--> bridge.py <--GET /state-- Noctalia 组件
```

桥只保留最近一次状态，不写磁盘。SPlayer-Next 插件每 5 秒发送一次心跳；超过 20 秒没有更新时，`/state` 的 `stale` 为 `true`，组件隐藏。

## 桥的生命周期

`bridge.luau` 是 Noctalia 插件的 `[[service]]` 条目，随插件启用而创建。

1. **启动**：`setsid -f nohup python3 <pluginDir>/bridge.py --port 33550 --owner-pid <noctalia pid> --bind-retries 20`。
   `setsid -f` 让桥脱离插件的进程树和终端；`runAsync` 立即返回，不需要 `& disown`。
2. **守护**：桥每秒读取 `--owner-pid` 对应进程的 `/proc/<pid>/stat`。Noctalia 退出、崩溃，或 PID 被复用时，`starttime` 不一致，桥自行退出。
3. **停止**：`onExit` 用一个 detached 的 Python 请求 `POST /quit`，失败重试 5 次。插件重载时也先停旧桥；新 runtime 用 `--bind-retries` 等待端口释放。
4. **兜底**：service 每 5 秒 `GET /health`，桥意外退出会重新拉起。

没有 systemd 服务。插件停用、Noctalia 退出后不会留下孤儿 `bridge.py`。

## 歌词行选择

SPlayer-Next 插件把当前行、当前行翻译、所属主行（背景/对唱时）、对唱行和下一行作为候选一起发送；Noctalia 组件按 `extra_mode` 的**回退路径**选择副行，按顺序取第一个可用的候选：

- `duet-translation-next`：对唱 → 翻译 → 下一行。
- `duet-translation`：对唱 → 翻译。
- `translation-next`：翻译 → 下一行。
- `duet-next`：对唱 → 下一行。
- `duet`：只显示对唱。
- `translation`：只显示翻译。
- `next`：只显示下一行。
- `none`：无副行。

主行规则：

1. 当前行是背景和声：主行用所属主行；和声行不进入副行候选。
2. 当前行是对唱且与上一条主行时间重叠：模式包含 `duet` 时主行用上一条主行；否则主行用当前对唱行。
3. 其他情况：主行用当前行。

对唱候选为空时，按模式里的下一项回退。过滤后的行不进入候选；当前行是制作信息时，主行回退到上一条正常歌词，副行只可能是它的翻译，不会使用制作署名。找不到上一条正常歌词时主行为空，组件回退到歌名。

## 制作人员过滤

SPlayer-Next 插件在 `lyricChange` 时计算一次 `creditFlags`。匹配规则：

- 行首是关键词 + `:` / `：`；
- 行内任意位置是关键词 + `:` / `：`；
- 行首是较长中文关键词 + 空格 + 内容；
- 英文角色允许关键词和冒号之间夹少量修饰词，如 `Music Copyist：`、`Produced by：`；
- 短行包含完整的歌手/专辑名时，也视为制作信息（例如 `W HOYO-MiX`、`Vinchi HOYO-MiX`，包括歌词段之后的署名行）；短名按完整词匹配，避免 `Nea` 命中 `beneath`/`near`。有翻译的歌词行不参与此判断。
- 前一行是带冒号的制作行，且后一行也是制作行或当前行是最后一行时，也视为拆行署名。

关键词来自 SPlayer-Next 插件设置，按字面匹配，逗号、分号、换行或 `|` 分隔。单字符关键词只认冒号形式，避免误伤「一曲」「曲终人散」这类歌词。

完整设置见 [CONFIGURATION.md](CONFIGURATION.md)。

## HTTP 接口

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/lyric`（别名 `/lyrics`、`/state`、`/update`） | 接收一次状态更新 |
| `GET` | `/state`（别名 `/lyric`、`/lyrics`） | 读取最新状态，附带 `ageMs`、`stale` |
| `GET` | `/health` | 健康检查 |
| `POST` | `/quit` | 停止桥 |
| `POST` | `/clear` | 清空状态 |
| `OPTIONS` | 任意 | CORS 预检 |

POST 字段：

```json
{
  "session": "随机字符串",
  "seq": 42,
  "playing": true,
  "title": "夜曲",
  "artist": "周杰伦",
  "album": "十一月的萧邦",
  "lyric": "一群嗜血的蚂蚁 被腐肉所吸引",
  "extra": "A swarm of bloodthirsty ants",
  "extraKind": "translation",
  "line": 12,
  "lineCount": 40,
  "positionMs": 45000,
  "durationMs": 227000,
  "updatedAt": 1794560000000
}
```

同一 `session` 内，`seq` 不大于已接受值的更新会被忽略，避免网络乱序导致状态倒退。`extraKind` 取 `translation`、`next`、`bg`、`duet` 或空字符串。

## 安全

桥默认只绑定 `127.0.0.1`，没有鉴权；任何本机进程都能读写歌词状态。不要将 `Bridge host` 改成 `0.0.0.0`。桥不接收文件、不执行命令、不落盘，POST 正文上限 64 KiB。
