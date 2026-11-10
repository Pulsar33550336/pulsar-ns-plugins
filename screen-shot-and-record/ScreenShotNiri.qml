import QtQuick
import Quickshell.Io
import qs.Commons

ScreenShot {
    id: root
    readonly property bool enableWindowsSelection: pluginApi?.pluginSettings?.enableWindowsSelection
                                                   ?? pluginApi?.manifest?.metadata?.defaultSettings?.enableWindowsSelection
                                                   ?? true

    property list<var> windowRegions: []
    property var hoveredWindow: null

    onEnableWindowsSelectionChanged: {
        if (!root.enableWindowsSelection) {
            root.windowRegions = []
            root.hoveredWindow = null
        }
    }

    onNonRecordingStart: () => {
        Logger.d("ScreenShot", "[Niri] onNonRecordingStart enableWindowsSelection=", root.enableWindowsSelection)
        if (root.enableWindowsSelection) {
            root.windowRegions = []
            niriWindowsProc.running = true
        }
    }

    onTargetChanged: {
        if (root.enableWindowsSelection && (root.target === "record" || root.target === "recordsound")) {
            root.windowRegions = []
            niriWindowsProc.running = true
        }
    }

    resolveFallbackRegion: () => {
        if (!root.hoveredWindow) {
            return null
        }
        return {
            x: root.hoveredWindow.x,
            y: root.hoveredWindow.y,
            width: root.hoveredWindow.width,
            height: root.hoveredWindow.height
        }
    }

    // niri ≥ 26.11（PR #4711）在 `niri msg windows` 的 WindowLayout 里为
    // 滚动布局的窗口提供了 tile_pos_in_workspace_view，坐标与旧的自定义
    // `window-geometries` 请求一致（相对当前 workspace view 的 tile 位置）。
    // 所以这里不再需要单独的 IPC 请求。
    Process {
        id: niriWindowsProc
        command: ["niri", "msg", "-j", "windows"]
        running: false
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const windows = JSON.parse(text)
                    root.windowRegions = root.computeRegions(windows)
                    Logger.d("ScreenShot", "[RegionSelector][Niri] Found", root.windowRegions.length, "windows on", root.screen?.name)
                } catch (e) {
                    root.windowRegions = []
                    Logger.w("ScreenShot", "[Niri] windows parse error:", e)
                }
            }
        }
        onExited: (code) => {
            if (code !== 0) {
                root.windowRegions = []
                Logger.w("ScreenShot", "[Niri] niriWindowsProc failed")
            }
        }
    }

    function computeRegions(windows) {
        if (!Array.isArray(windows)) {
            Logger.w("ScreenShot", "[Niri] computeRegions: windows is not an array")
            return []
        }

        const focused = windows.find((w) => w.is_focused === true)
        if (!focused) {
            Logger.d("ScreenShot", "[Niri] computeRegions: no focused window")
            return []
        }
        const activeWsId = Number(focused.workspace_id ?? 0)

        const outputWidth = Number(root.screen?.width ?? 0)
        const outputHeight = Number(root.screen?.height ?? 0)

        const result = []
        for (const win of windows) {
            if (Number(win.workspace_id ?? 0) !== activeWsId) continue

            const l = win.layout
            // 不在 view 内的 tile 该字段为 null；可见（含部分可见）的才有值。
            const tilePos = l?.tile_pos_in_workspace_view
            if (!Array.isArray(tilePos)) continue

            const tileX = Number(tilePos[0] ?? 0)
            const tileY = Number(tilePos[1] ?? 0)
            const winOffX = Number(l.window_offset_in_tile[0] ?? 0)
            const winOffY = Number(l.window_offset_in_tile[1] ?? 0)
            const winW = Number(l.window_size[0] ?? 0)
            const winH = Number(l.window_size[1] ?? 0)

            if (winW <= 0 || winH <= 0) continue

            const visX = tileX + winOffX
            const visY = tileY + winOffY

            if (visX + winW < 0 || visX > outputWidth || visY + winH < 0 || visY > outputHeight) continue

            result.push({
                x: Math.round(visX) - root.monitorOffsetX,
                y: Math.round(visY) - root.monitorOffsetY,
                width: Math.round(winW),
                height: Math.round(winH),
                title: String(win.title ?? ""),
                cls: String(win.app_id ?? ""),
                address: String(win.id ?? ""),
                floating: win.is_floating === true
            })
        }

        result.sort((a, b) => Number(a.floating) - Number(b.floating))

        return result
    }

    function findWindowAt(x, y) {
        for (let i = root.windowRegions.length - 1; i >= 0; i--) {
            const w = root.windowRegions[i]
            if (x >= w.x && x <= w.x + w.width && y >= w.y && y <= w.y + w.height) {
                return w
            }
        }
        return null
    }
}
