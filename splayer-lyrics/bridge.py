#!/usr/bin/env python3
"""在 SPlayer-Next 与 Noctalia 之间转发当前歌词的本地 HTTP 桥。

SPlayer-Next 插件 POST 当前歌词，Noctalia 组件 GET /state。只保留最近一次状态，
不落盘；超过 ``--stale-after`` 秒没有更新时，快照标记为 stale。
"""

from __future__ import annotations

import argparse
import errno
import json
import math
import signal
import sys
import threading
import time
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import urlsplit

VERSION = "1.0.0"
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 33550
DEFAULT_STALE_AFTER = 20.0

# POST 正文上限；歌词行很短，留足空间给长句和翻译。
MAX_BODY_BYTES = 64 * 1024
MAX_TEXT_CHARS = 2000
MAX_SESSION_CHARS = 64

EXTRA_KINDS = {"", "translation", "next", "bg", "duet"}

POST_PATHS = {"/lyric", "/lyrics", "/state", "/update"}
STATE_PATHS = {"/state", "/lyric", "/lyrics", "/update"}


def _as_text(value: Any, limit: int = MAX_TEXT_CHARS) -> str:
    """把任意 JSON 值收敛成有长度上限的字符串。"""
    if isinstance(value, str):
        return value[:limit].strip()
    return ""


def _as_number(value: Any, limit: float = 86_400_000.0) -> float:
    """把任意 JSON 值收敛成 [0, limit] 内的有限数。"""
    try:
        number = float(value)
    except (TypeError, ValueError):
        return 0.0
    if not math.isfinite(number):
        return 0.0
    return min(max(number, 0.0), limit)


def _as_bool(value: Any) -> bool:
    return value is True


def _proc_starttime(pid: int) -> str | None:
    """读取 /proc/<pid>/stat 的 starttime 字段。

    只拿 PID 不够：Noctalia 退出后 PID 可能被复用。starttime 在进程生命周期内
    不变，复用 PID 时一定会变，所以用它做身份校验。
    """
    try:
        with open(f"/proc/{pid}/stat", "rb") as handle:
            raw = handle.read().decode("utf-8", "replace")
    except OSError:
        return None
    close = raw.rfind(")")
    if close < 0:
        return None
    # ) 之后从 state(第 3 字段) 开始；starttime 是第 22 字段，即这里的第 19 项。
    fields = raw[close + 2 :].split()
    if len(fields) < 20:
        return None
    return fields[19]


class LyricState:
    """最近一次歌词状态，线程安全。

    并发 POST 可能乱序到达：同一 session 内 ``seq`` 不大于已接受值的更新会被
    忽略，避免状态倒退。
    """

    def __init__(self, stale_after: float) -> None:
        self._lock = threading.Lock()
        self._stale_after_ms = max(stale_after, 1.0) * 1000.0
        self._session = ""
        self._seq = -1
        self._received_at = 0.0
        self._data: dict[str, Any] = {
            "playing": False,
            "title": "",
            "artist": "",
            "album": "",
            "lyric": "",
            "extra": "",
            "extraKind": "",
            "protocol": 0,
            "current": "",
            "currentTranslation": "",
            "bgMain": "",
            "bgMainTranslation": "",
            "duetMain": "",
            "duetMainTranslation": "",
            "background": "",
            "duet": "",
            "next": "",
            "isBG": False,
            "isDuet": False,
            "duetOverlap": False,
            "line": -1,
            "lineCount": 0,
            "positionMs": 0,
            "durationMs": 0,
            "updatedAt": 0,
        }

    def update(self, incoming: dict[str, Any]) -> tuple[bool, bool]:
        """接受一条更新，返回 ``(accepted, duplicate)``。"""
        session = _as_text(incoming.get("session"), MAX_SESSION_CHARS)
        try:
            seq = int(incoming.get("seq", -1))
        except (TypeError, ValueError):
            seq = -1

        with self._lock:
            if session and seq >= 0 and session == self._session and seq <= self._seq:
                return False, True
            if session != self._session:
                self._session = session
                self._seq = -1
            if seq > self._seq:
                self._seq = seq

            extra_kind = _as_text(incoming.get("extraKind"), 24)
            if extra_kind not in EXTRA_KINDS:
                extra_kind = ""

            line = incoming.get("line", -1)
            try:
                line = int(line)
            except (TypeError, ValueError):
                line = -1
            line_count = incoming.get("lineCount", 0)
            try:
                line_count = max(0, int(line_count))
            except (TypeError, ValueError):
                line_count = 0

            self._data = {
                "playing": _as_bool(incoming.get("playing")),
                "title": _as_text(incoming.get("title")),
                "artist": _as_text(incoming.get("artist")),
                "album": _as_text(incoming.get("album")),
                "lyric": _as_text(incoming.get("lyric")),
                "extra": _as_text(incoming.get("extra")),
                "extraKind": extra_kind,
                "protocol": int(_as_number(incoming.get("protocol"))),
                "current": _as_text(incoming.get("current")),
                "currentTranslation": _as_text(incoming.get("currentTranslation")),
                "bgMain": _as_text(incoming.get("bgMain")),
                "bgMainTranslation": _as_text(incoming.get("bgMainTranslation")),
                "duetMain": _as_text(incoming.get("duetMain")),
                "duetMainTranslation": _as_text(incoming.get("duetMainTranslation")),
                "background": _as_text(incoming.get("background")),
                "duet": _as_text(incoming.get("duet")),
                "next": _as_text(incoming.get("next")),
                "isBG": _as_bool(incoming.get("isBG")),
                "isDuet": _as_bool(incoming.get("isDuet")),
                "duetOverlap": _as_bool(incoming.get("duetOverlap")),
                "line": line,
                "lineCount": line_count,
                "positionMs": int(_as_number(incoming.get("positionMs"))),
                "durationMs": int(_as_number(incoming.get("durationMs"))),
                "updatedAt": int(
                    _as_number(
                        incoming.get("updatedAt"), time.time() * 1000 + 86_400_000
                    )
                ),
            }
            self._received_at = time.time()
            return True, False

    def snapshot(self) -> dict[str, Any]:
        """返回给读取方的副本，附带年龄和是否过期。"""
        with self._lock:
            data = dict(self._data)
            received_at = self._received_at
            session = self._session
            seq = self._seq

        now = time.time()
        has_state = received_at > 0
        age_ms = int(max(0.0, now - received_at) * 1000.0) if has_state else None
        stale = (not has_state) or (
            age_ms is not None and age_ms > self._stale_after_ms
        )

        data.update(
            {
                "ok": True,
                "version": VERSION,
                "session": session,
                "seq": seq,
                "hasState": has_state,
                "stale": stale,
                "ageMs": age_ms,
                "serverNow": int(now * 1000.0),
                "staleAfterMs": int(self._stale_after_ms),
            }
        )
        return data

    def clear(self) -> None:
        with self._lock:
            self._session = ""
            self._seq = -1
            self._received_at = 0.0
            self._data.update(
                {
                    "playing": False,
                    "title": "",
                    "artist": "",
                    "album": "",
                    "lyric": "",
                    "extra": "",
                    "extraKind": "",
                    "protocol": 0,
                    "current": "",
                    "currentTranslation": "",
                    "bgMain": "",
                    "bgMainTranslation": "",
                    "duetMain": "",
                    "duetMainTranslation": "",
                    "background": "",
                    "duet": "",
                    "next": "",
                    "isBG": False,
                    "isDuet": False,
                    "duetOverlap": False,
                    "line": -1,
                    "lineCount": 0,
                    "positionMs": 0,
                    "durationMs": 0,
                    "updatedAt": 0,
                }
            )


class BridgeServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def __init__(
        self, address: tuple[str, int], state: LyricState, verbose: bool = False
    ) -> None:
        super().__init__(address, BridgeHandler)
        self.state = state
        self.verbose = verbose


class BridgeHandler(BaseHTTPRequestHandler):
    server_version = f"SplayerLyricsBridge/{VERSION}"
    protocol_version = "HTTP/1.1"
    server: BridgeServer

    def _send(self, status: int, payload: bytes, content_type: str) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, Accept")
        self.send_header("Access-Control-Allow-Private-Network", "true")
        self.end_headers()
        if self.command != "HEAD" and payload:
            self.wfile.write(payload)

    def _send_json(self, status: int, value: dict[str, Any]) -> None:
        payload = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode(
            "utf-8"
        )
        self._send(status, payload, "application/json; charset=utf-8")

    def log_message(self, fmt: str, *args: Any) -> None:
        if self.server.verbose:
            super().log_message(fmt, *args)

    def do_OPTIONS(self) -> None:
        self._send(HTTPStatus.NO_CONTENT, b"", "text/plain; charset=utf-8")

    def do_GET(self) -> None:
        path = urlsplit(self.path).path
        if path in STATE_PATHS:
            self._send_json(HTTPStatus.OK, self.server.state.snapshot())
            return
        if path == "/health":
            snapshot = self.server.state.snapshot()
            self._send_json(
                HTTPStatus.OK,
                {
                    "ok": True,
                    "version": VERSION,
                    "hasState": snapshot["hasState"],
                    "stale": snapshot["stale"],
                    "playing": snapshot["playing"],
                    "title": snapshot["title"],
                    "ageMs": snapshot["ageMs"],
                },
            )
            return
        self._send_json(HTTPStatus.NOT_FOUND, {"ok": False, "error": "not found"})

    def do_POST(self) -> None:
        path = urlsplit(self.path).path
        if path == "/quit":
            self._send_json(HTTPStatus.OK, {"ok": True, "quitting": True})
            # 从 handler 线程停 serve_forever 是安全的；直接在 serve_forever 线程
            # 里调用 shutdown() 会死锁。
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return
        if path == "/clear":
            self.server.state.clear()
            self._send_json(HTTPStatus.OK, {"ok": True, "cleared": True})
            return
        if path not in POST_PATHS:
            self._send_json(HTTPStatus.NOT_FOUND, {"ok": False, "error": "not found"})
            return

        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            length = 0
        if length <= 0:
            self._send_json(
                HTTPStatus.BAD_REQUEST, {"ok": False, "error": "empty body"}
            )
            return
        if length > MAX_BODY_BYTES:
            self.close_connection = True
            self._send_json(
                HTTPStatus.REQUEST_ENTITY_TOO_LARGE,
                {"ok": False, "error": "body too large"},
            )
            return

        body = self.rfile.read(length)
        try:
            incoming = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            self._send_json(
                HTTPStatus.BAD_REQUEST, {"ok": False, "error": "invalid json"}
            )
            return
        if not isinstance(incoming, dict):
            self._send_json(
                HTTPStatus.BAD_REQUEST, {"ok": False, "error": "object expected"}
            )
            return

        accepted, duplicate = self.server.state.update(incoming)
        self._send_json(
            HTTPStatus.OK,
            {"ok": True, "accepted": accepted, "duplicate": duplicate},
        )


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="SPlayer-Next 歌词到 Noctalia 的本地 HTTP 桥",
    )
    parser.add_argument(
        "--host", default=DEFAULT_HOST, help=f"监听地址（默认 {DEFAULT_HOST}）"
    )
    parser.add_argument(
        "--port",
        type=int,
        default=DEFAULT_PORT,
        help=f"监听端口（默认 {DEFAULT_PORT}）",
    )
    parser.add_argument(
        "--stale-after",
        type=float,
        default=DEFAULT_STALE_AFTER,
        metavar="SECONDS",
        help=f"多久没有更新就视为过期（默认 {DEFAULT_STALE_AFTER:g} 秒）",
    )
    parser.add_argument(
        "--owner-pid",
        type=int,
        default=0,
        metavar="PID",
        help="Noctalia 进程 PID；该进程退出或 PID 被复用时，桥自动退出",
    )
    parser.add_argument(
        "--bind-retries",
        type=int,
        default=0,
        metavar="N",
        help="端口被占用时重试绑定多少次（默认 0）",
    )
    parser.add_argument(
        "--bind-retry-delay",
        type=float,
        default=0.5,
        metavar="SECONDS",
        help="每次绑定重试之间的间隔（默认 0.5 秒）",
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="打印每条 HTTP 请求"
    )
    return parser.parse_args(argv)


def _watch_owner(server: BridgeServer, pid: int, starttime: str) -> None:
    """Noctalia 退出后关闭桥；PID 复用通过 starttime 识别。"""
    while True:
        time.sleep(1.0)
        if _proc_starttime(pid) != starttime:
            threading.Thread(target=server.shutdown, daemon=True).start()
            return


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    if not 1 <= args.port <= 65535:
        print(f"error: port out of range: {args.port}", file=sys.stderr)
        return 2

    owner_start: str | None = None
    if args.owner_pid > 0:
        owner_start = _proc_starttime(args.owner_pid)
        if owner_start is None:
            print(f"error: owner pid {args.owner_pid} not found", file=sys.stderr)
            return 2

    state = LyricState(args.stale_after)
    attempts = max(0, args.bind_retries)
    server: BridgeServer | None = None
    for attempt in range(attempts + 1):
        try:
            server = BridgeServer((args.host, args.port), state, verbose=args.verbose)
            break
        except OSError as exc:
            if exc.errno != errno.EADDRINUSE or attempt >= attempts:
                print(
                    f"error: cannot listen on {args.host}:{args.port}: {exc}",
                    file=sys.stderr,
                )
                return 1
            time.sleep(max(0.05, args.bind_retry_delay))
    assert server is not None

    if owner_start is not None:
        threading.Thread(
            target=_watch_owner,
            args=(server, args.owner_pid, owner_start),
            daemon=True,
        ).start()

    def stop(_signum: int, _frame: Any) -> None:
        # shutdown() 会等待 serve_forever 退出，不能在同一个线程里直接调用。
        threading.Thread(target=server.shutdown, daemon=True).start()

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)

    owner = f", owner pid {args.owner_pid}" if args.owner_pid > 0 else ""
    print(
        f"splayer-lyrics-bridge {VERSION} listening on http://{args.host}:{args.port} "
        f"(POST /lyric, GET /state, stale after {args.stale_after:g}s{owner})",
        file=sys.stderr,
        flush=True,
    )
    try:
        server.serve_forever(poll_interval=0.5)
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
