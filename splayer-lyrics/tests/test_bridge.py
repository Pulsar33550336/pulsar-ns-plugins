#!/usr/bin/env python3
"""bridge.py 的回归测试：只依赖标准库，直接跑 `python3 tests/test_bridge.py`。"""

from __future__ import annotations

import importlib.util
import json
import threading
import time
import unittest
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("splayer_bridge", ROOT / "bridge.py")
bridge = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(bridge)


class BridgeTest(unittest.TestCase):
    def setUp(self) -> None:
        self.state = bridge.LyricState(stale_after=2.0)
        self.server = bridge.BridgeServer(("127.0.0.1", 0), self.state)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        host, port = self.server.server_address[:2]
        self.base = f"http://{host}:{port}"

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)

    def request(self, path: str, method: str = "GET", payload: dict | None = None):
        data = None
        headers = {}
        if payload is not None:
            data = json.dumps(payload).encode("utf-8")
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(self.base + path, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=2) as response:
                return response.status, json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as error:
            return error.code, json.loads(error.read().decode("utf-8"))

    def post(self, payload: dict):
        return self.request("/lyric", "POST", payload)

    def test_initial_state_is_stale(self) -> None:
        status, body = self.request("/state")
        self.assertEqual(status, 200)
        self.assertFalse(body["hasState"])
        self.assertTrue(body["stale"])
        self.assertIsNone(body["ageMs"])

    def test_accepts_update_and_reports_fields(self) -> None:
        status, body = self.post(
            {
                "session": "s1",
                "seq": 1,
                "playing": True,
                "title": "Song",
                "artist": "Artist",
                "lyric": "line",
                "extra": "翻译",
                "extraKind": "translation",
                "line": 3,
                "lineCount": 10,
                "positionMs": 1234.6,
                "durationMs": 200000,
            }
        )
        self.assertEqual(status, 200)
        self.assertTrue(body["accepted"])

        _, state = self.request("/state")
        self.assertTrue(state["hasState"])
        self.assertFalse(state["stale"])
        self.assertTrue(state["playing"])
        self.assertEqual(state["title"], "Song")
        self.assertEqual(state["artist"], "Artist")
        self.assertEqual(state["lyric"], "line")
        self.assertEqual(state["extra"], "翻译")
        self.assertEqual(state["extraKind"], "translation")
        self.assertEqual(state["line"], 3)
        self.assertEqual(state["lineCount"], 10)
        self.assertEqual(state["positionMs"], 1234)

    def test_stores_lyric_candidates(self) -> None:
        self.post(
            {
                "session": "s1",
                "seq": 1,
                "protocol": 2,
                "current": "current",
                "currentTranslation": "当前翻译",
                "bgMain": "main",
                "bgMainTranslation": "主行翻译",
                "duetMain": "duet main",
                "duetMainTranslation": "对唱主行翻译",
                "background": "bg",
                "duet": "duet",
                "next": "next",
                "isBG": False,
                "isDuet": True,
                "duetOverlap": True,
            }
        )
        _, state = self.request("/state")
        self.assertEqual(state["protocol"], 2)
        self.assertEqual(state["current"], "current")
        self.assertEqual(state["currentTranslation"], "当前翻译")
        self.assertEqual(state["bgMain"], "main")
        self.assertEqual(state["bgMainTranslation"], "主行翻译")
        self.assertEqual(state["duetMain"], "duet main")
        self.assertEqual(state["duetMainTranslation"], "对唱主行翻译")
        self.assertEqual(state["background"], "bg")
        self.assertEqual(state["duet"], "duet")
        self.assertEqual(state["next"], "next")
        self.assertFalse(state["isBG"])
        self.assertTrue(state["isDuet"])
        self.assertTrue(state["duetOverlap"])

    def test_older_sequence_is_ignored(self) -> None:
        first = {"session": "s1", "seq": 5, "title": "new"}
        second = {"session": "s1", "seq": 4, "title": "old"}
        self.assertTrue(self.post(first)[1]["accepted"])
        _, body = self.post(second)
        self.assertTrue(body["duplicate"])
        self.assertFalse(body["accepted"])
        _, state = self.request("/state")
        self.assertEqual(state["title"], "new")
        self.assertEqual(state["seq"], 5)

    def test_new_session_resets_sequence(self) -> None:
        self.post({"session": "old", "seq": 99, "title": "old"})
        _, body = self.post({"session": "new", "seq": 1, "title": "new"})
        self.assertTrue(body["accepted"])
        _, state = self.request("/state")
        self.assertEqual(state["title"], "new")

    def test_state_becomes_stale(self) -> None:
        self.post({"session": "s1", "seq": 1, "playing": True, "title": "Song"})
        time.sleep(2.2)
        _, state = self.request("/state")
        self.assertTrue(state["stale"])
        self.assertGreaterEqual(state["ageMs"], 2000)

    def test_invalid_json_is_rejected(self) -> None:
        req = urllib.request.Request(
            self.base + "/lyric",
            data=b"{not json",
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with self.assertRaises(urllib.error.HTTPError) as caught:
            urllib.request.urlopen(req, timeout=2)
        self.assertEqual(caught.exception.code, 400)
        caught.exception.close()

    def test_oversized_body_is_rejected(self) -> None:
        req = urllib.request.Request(
            self.base + "/lyric",
            data=b"{}" + b" " * (bridge.MAX_BODY_BYTES + 10),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with self.assertRaises(urllib.error.HTTPError) as caught:
            urllib.request.urlopen(req, timeout=2)
        self.assertEqual(caught.exception.code, 413)
        caught.exception.close()

    def test_quit_endpoint(self) -> None:
        status, body = self.request("/quit", "POST", {})
        self.assertEqual(status, 200)
        self.assertTrue(body["quitting"])
        self.thread.join(timeout=2)
        self.assertFalse(self.thread.is_alive())

    def test_clear(self) -> None:
        self.post({"session": "s1", "seq": 1, "playing": True, "title": "Song"})
        status, body = self.request("/clear", "POST", {})
        self.assertEqual(status, 200)
        self.assertTrue(body["cleared"])
        _, state = self.request("/state")
        self.assertFalse(state["hasState"])
        self.assertTrue(state["stale"])

    def test_health_and_options(self) -> None:
        status, body = self.request("/health")
        self.assertEqual(status, 200)
        self.assertTrue(body["ok"])
        self.assertEqual(body["version"], bridge.VERSION)

        req = urllib.request.Request(self.base + "/state", method="OPTIONS")
        with urllib.request.urlopen(req, timeout=2) as response:
            self.assertEqual(response.status, 204)
            self.assertEqual(response.headers["Access-Control-Allow-Origin"], "*")


if __name__ == "__main__":
    unittest.main(verbosity=2)
