--!nonstrict
-- ScreenShot & Record — IPC surface.
--
-- `onIpc` on a `[[service]]` is the one entry that is always live: panels and bar
-- widgets only exist when they are on screen, so a keybind aimed at them can hit
-- "matched plugin entry is not ready" (panels) or "matched multiple plugin
-- instances" (two bar widgets of the same entry). A service is a singleton and
-- is instantiated as soon as the plugin is enabled, which is what a keybind
-- wants:
--
--   noctalia msg plugin pulsar/screen-shot-and-record:service all screenshot
--   noctalia msg plugin pulsar/screen-shot-and-record:service all ocr
--   noctalia msg plugin pulsar/screen-shot-and-record:service all stoprecord
--
-- Same verbs as the widget and panel entries; `lib/actions.luau` owns what each
-- one does, so all three surfaces stay in sync.

local actions = require("./lib/actions.luau")

local IPC = {
    screenshot = true,
    ocr = true,
    search = true,
    record = true,
    recordsound = true,
}

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
