--!nonstrict
-- ScreenShot & Record — bar icon.
--
-- The icon opens the plugin's own panel, which is the target list; picking a
-- target there is what actually starts the overlay. That is the QML plugin's
-- shape too (`BarWidget.qml` called `pluginApi.openPanel`), with the panel as a
-- `[[panel]]` entry instead of `Panel.qml`.
--
-- Two things this entry adds on its own:
--
--   * the glyph turns `error`-coloured while `wf-recorder` is running, so a
--     recording in progress is visible from the bar (QML bound the same fact to
--     `recordingActive` and turned the whole capsule red);
--   * `onIpc` — a keybind can start a target without touching the bar, which is
--     what the QML plugin's `IpcHandler` was for:
--
--       noctalia msg plugin pulsar/screen-shot-and-record:widget all screenshot
--       noctalia msg plugin pulsar/screen-shot-and-record:widget all ocr
--       noctalia msg plugin pulsar/screen-shot-and-record:widget all stoprecord
--
-- Only `update`, `onClick` and `onIpc` are defined: `onRightClick` and
-- `onMiddleClick` are left alone, so the user can bind them, and middle click
-- keeps the built-in "open this widget's settings" binding.

local actions = require("./lib/actions.luau")

local PANEL = "pulsar/screen-shot-and-record:panel"
local RECORDER = "wf-recorder"
local GLYPH = "screenshot"

-- Targets `onIpc` accepts, plus the one verb that is not a target.
local IPC = {
    screenshot = true,
    ocr = true,
    search = true,
    record = true,
    recordsound = true,
}

-- A recording is the one state this widget shows, so it is worth re-checking
-- rather than caching: the recorder can also be stopped from a terminal, and a
-- glyph stuck on "recording" would be a lie. `processMatches` reads a cached
-- /proc scan, so a two-second tick is not a scan every two seconds.
local function applyRecording(recording)
    if recording then
        barWidget.setGlyphColor("error")
    else
        barWidget.setGlyphColor("on_surface")
    end
end

function update()
    noctalia.setUpdateInterval(2000)
    barWidget.setGlyph(GLYPH)
    barWidget.setTooltip(noctalia.tr("tooltip"))
    applyRecording(noctalia.state.get("recording") == true)
    noctalia.processMatches(function(running)
        applyRecording(running)
    end, RECORDER)
end

function onClick()
    noctalia.togglePanel(PANEL)
end

function onIpc(event)
    if event == "stoprecord" then
        actions.stopRecording()
        return
    end
    if IPC[event] then
        actions.run(event)
        return
    end
    noctalia.log("screen-shot-and-record: unknown ipc event " .. tostring(event))
end

-- No trailing `update()`: the host calls it once as soon as the chunk has
-- loaded, then on the interval set above.
