# 排错

## 组件不出现或一直是空白

1. `pgrep -af 'splayer-lyrics/.*bridge.py'` 看桥是否在跑；
2. `curl -s http://127.0.0.1:33550/health` 是否返回 `ok: true`；
3. `/state` 里 `hasState` 是否为 `true`、`stale` 是否为 `false`；
4. `playing` 是否为 `true`；暂停时默认隐藏，可打开「暂停时保留歌词」；
5. 歌曲是否有歌词；没有时默认显示歌名，纯音乐也会显示歌名；
6. Noctalia 日志里是否有 `plugin pulsar/splayer-lyrics:bridge` 的报错：

   ```bash
   tail -f ~/.cache/noctalia/noctalia.log | grep splayer
   ```

## 桥没有自动起来

- 确认插件已启用：`noctalia msg plugins list | grep splayer`；
- 看 Noctalia 日志里的 `splayer-lyrics`；
- 查询 service 状态：

  ```bash
  noctalia msg plugin pulsar/splayer-lyrics:bridge all status
  ```

  这里会打印 bridge 路径、host/port、owner pid；
- 确认 `python3` 在 Noctalia 的 `PATH` 里；
- 手动运行排查：

  ```bash
  python3 ~/.local/share/noctalia/plugins/splayer-lyrics/bridge.py --port 33550 -v
  ```

## Noctalia 报 unrecognized widget type

```text
unrecognized widget type "pulsar/splayer-lyrics:widget"
```

插件还没启用，或者配置先于插件加载。先启用插件，再执行：

```bash
noctalia msg config-reload
```

仍有问题就重启 Noctalia。

## SPlayer-Next 插件是 Error 状态

- 查看插件卡片上的错误；
- 需要 SPlayer-Next 插件 API level >= 2（当前版本为 5）；
- 端口被占用时插件不会报错（请求失败被静默处理），先检查桥。

## 端口被占用

```bash
ss -ltnp | grep 33550
```

改端口时同步修改 Noctalia 的 **Bridge port**、SPlayer-Next 的 **桥端口**，以及手动运行时的 `--port`。

## 桥禁用后进程还在

正常路径下 `onExit` 会请求 `/quit`，`--owner-pid` 看门狗会在 Noctalia 退出后 1 秒内收掉。若仍有残留（例如手动启动过）：

```bash
pkill -f 'splayer-lyrics/.*bridge.py'
```

## 手动安装

```bash
cd ~/Projects/noctalia-splayer-lyrics
mkdir -p ~/.local/share/noctalia/plugins/splayer-lyrics
cp plugin.toml bridge.py bridge.luau widget.luau ~/.local/share/noctalia/plugins/splayer-lyrics/
cp -R translations ~/.local/share/noctalia/plugins/splayer-lyrics/
noctalia msg plugins enable pulsar/splayer-lyrics
noctalia msg config-reload
```

开发时可以把插件目录软链过去，改 `.luau` 会热重载：

```bash
rm -rf ~/.local/share/noctalia/plugins/splayer-lyrics
ln -s "$PWD" ~/.local/share/noctalia/plugins/splayer-lyrics
```

`bridge.py` 必须跟着插件目录一起存在；service 用 `noctalia.pluginDir()` 查找它。

## 测试

```bash
python3 tests/test_bridge.py
node tests/test_splayer_plugin.js
noctalia plugins lint .
```
