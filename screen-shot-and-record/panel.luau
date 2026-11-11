--!nonstrict
-- ScreenShot & Record — the target panel.
--
-- This is the v5 spelling of the QML plugin's `Panel.qml`: a row per target,
-- where the target is what the overlay's bar will say and what `lib/actions.luau`
-- will do once the selection is finished. Plus one extra row — "stop recording" —
-- which replaces the two recording targets while `wf-recorder` is running,
-- exactly as the QML panel swapped its button set on `pidof wf-recorder`.
--
-- The panel declares its UI as a `ui.*` tree; Noctalia diffs it against the
-- previous one, so re-rendering an unchanged panel costs nothing. Clicking a
-- target closes the panel *first* — the overlay takes over the whole screen, and
-- a panel left open would sit on top of it.

local actions = require("./lib/actions.luau")

local RECORDER = "wf-recorder"

-- The target list lives in `lib/actions.luau`, beside what each target *does*:
-- the same five ids and the same glyphs, so the panel row and the overlay's bar
-- cannot drift apart.
local TARGETS = actions.targets()

-- The verbs `onIpc` accepts, built from the same list the rows are drawn from.
local IPC = {}
for _, target in ipairs(TARGETS) do
    IPC[target.id] = true
end

-- Whether a recording is live. Corrected on every open rather than trusted from
-- the previous one: the recorder can also be stopped from a terminal.
local recording = false

-- The target the user picked, waiting for the panel to be gone.
--
-- The launch is deliberately **not** done on the click. `Panel.qml` did the same
-- thing — the button set `root.target` and `Component.onDestruction` called
-- `mainInstance.open(target)` — and the reason is the overlay itself: it takes
-- its frozen frame from a screencopy of the output, so a panel still on screen
-- would be *in the screenshot* the user is about to select from, under the
-- overlay's own dimming. `panel.close()` only asks; `onClose` is the host
-- reporting that it has torn the surface down, and it is the earliest moment the
-- overlay can go up. It is a real signal, not a hint: `PanelManager::destroyPanel`
-- resets the scene root and the layer surface around the `onClose()` call, so by
-- the time this callback runs the surface is already gone.
local pending = nil

local function choose(target)
    pending = target
    panel.close()
end

-- Stopping a recording has no overlay to wait for — it is a `pkill` and a
-- notification — so unlike a target it runs on the click.
local function stop()
    panel.close()
    actions.stopRecording()
end

local function render()
    local rows = {
        ui.row({ align = "center", gap = 8 }, {
            ui.glyph({ name = "screenshot", size = 20, color = "primary" }),
            ui.label({
                text = noctalia.tr("panel.title"),
                fontSize = 16,
                fontWeight = "bold",
                color = "on_surface",
                flexGrow = 1,
            }),
        }),
    }

    for _, target in ipairs(TARGETS) do
        -- The recording targets are hidden while a recording is live: recording
        -- again is not a thing, and the row below is what the user wants.
        if not recording or (target.id ~= "record" and target.id ~= "recordsound") then
            table.insert(rows, ui.button({
                -- Stable identity per target: the reconciler keeps a node whose
                -- key is unchanged, so the click handler is not re-created on a
                -- re-render (which is what the recording check triggers).
                key = target.id,
                glyph = target.glyph,
                text = noctalia.tr("target." .. target.id),
                variant = "primary",
                -- No `contentAlign`: the control's own default is `Center`
                -- (`Button::m_contentAlign`, src/ui/controls/button.h), which is
                -- what `Panel.qml`'s `NButton` did too — it never set one. An
                -- explicit "start" left-aligned the glyph and label, which is the
                -- one thing the QML panel did not look like.
                onClick = function()
                    choose(target.id)
                end,
            }))
        end
    end

    if recording then
        table.insert(rows, ui.button({
            glyph = "close",
            text = noctalia.tr("panel.stop"),
            variant = "destructive",
            onClick = stop,
        }))
    end

    panel.render(ui.column({ gap = 8 }, rows))
end

function onOpen(_context)
    -- Render the last known state first: the check below is asynchronous, and a
    -- panel that opens with no rows while it asks looks broken. The rows are then
    -- corrected if the answer disagrees — including the first time, when nothing
    -- is known yet.
    render()
    noctalia.processMatches(function(running)
        if running ~= recording then
            recording = running
            render()
        end
    end, RECORDER)
end

-- The panel is gone: now the overlay can go up. See `pending` above — this is the
-- port of `Panel.qml`'s `Component.onDestruction`. Every other way of closing the
-- panel (outside click, Escape, a reload) leaves `pending` nil and does nothing,
-- which is what `if (target != "")` did.
function onClose()
    local target = pending
    pending = nil
    if target ~= nil then
        actions.run(target)
    end
end

-- IPC, so a keybind can start a target without the panel being open (and without
-- a bar widget being present). Same verbs as `widget.luau`'s `onIpc`, which is
-- where the QML plugin's `IpcHandler` went:
--
--   noctalia msg plugin pulsar/screen-shot-and-record:panel all screenshot
--   noctalia msg plugin pulsar/screen-shot-and-record:panel all stoprecord
--
-- Unlike a click, this runs the target directly: there is no panel surface to
-- tear down, and `actions.run` opens the overlay itself.
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
