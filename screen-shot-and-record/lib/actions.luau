--!nonstrict
-- What a finished selection *means*.
--
-- The overlay captures, writes one PNG and reports which mouse button ended the
-- selection. Everything else is policy, and it lives here: which target runs
-- which tool, where files go, what the user is told. This is the port of the
-- QML plugin's `capture.sh` / `record.sh`, of `Panel.qml`'s target list, and of
-- `Main.qml`'s recording toggle.
--
-- Shared by both entries, so a target behaves identically whether it was picked
-- in the panel or triggered over IPC.

local overlay = require("./overlay.luau")

local M = {}

local RECORDER = "wf-recorder"
local CAPTURE_SCRIPT = "capture.sh"

-- The target list: the ids, in the order the panel draws them, and the Tabler
-- glyph each one shows — in the panel row *and* on the overlay's bar.
--
-- One table, in one place, because both uses are the same fact. The binary has no
-- target concept at all: it draws whatever `--target-icon` names, so the choice of
-- glyph is this module's, and `launch` is told which one. The QML plugin kept the
-- same mapping in `iconForTarget` (`ScreenShot.qml:179-199`); these are the Tabler
-- entries it resolved to.
--
-- `image = false` says the target has no use for the crop: a recording needs the
-- *rectangle* (the compositor records the live screen) and never looks at the
-- pixels, so the overlay is told `--region-only` and skips the crop and the PNG
-- encode entirely. This is the only place that fact lives, beside the action it
-- belongs to.
local TARGETS = {
    { id = "screenshot", glyph = "screenshot" },
    { id = "ocr", glyph = "text-recognition" },
    { id = "search", glyph = "photo-search" },
    { id = "record", glyph = "camera", image = false },
    { id = "recordsound", glyph = "camera-spark", image = false },
}

-- id -> glyph, for the lookups `M.run` and `M.capture` need. Built from the list
-- above so the two cannot disagree.
local GLYPH = {}
for _, target in ipairs(TARGETS) do
    GLYPH[target.id] = target.glyph
end

-- id -> whether `-o` is worth passing. Absent means yes: every target but the
-- recording ones is handed a file.
local IMAGE = {}
for _, target in ipairs(TARGETS) do
    IMAGE[target.id] = target.image ~= false
end

-- The target list, for the panel to draw.
function M.targets()
    return TARGETS
end

-- Tools a target needs. Checked *before* the overlay opens, which is a
-- deliberate improvement on the QML plugin: it discovered a missing tesseract
-- after the user had already selected a region, and the selection was lost.
-- `screenshot` is absent because which tool it needs is decided by the mouse
-- button, which nobody knows until the selection is over.
local TARGET_TOOLS = {
    ocr = { "tesseract", "wl-copy" },
    search = { "curl", "jq" },
    record = { RECORDER },
    recordsound = { RECORDER },
}

-- Outcome token from `capture.sh` -> the notification it earns. `saved` has no
-- fixed body: the point of that notification is the path the editor wrote.
local OUTCOMES = {
    copied = { title = "copied.title", body = "copied.body" },
    saved = { title = "saved.title" },
    ["ocr-copied"] = { title = "ocr.done-title", body = "ocr.copied-body" },
    ["ocr-empty"] = { title = "ocr.done-title", body = "ocr.empty-body" },
    searched = { title = "search.done-title", body = "search.opened-body" },
}

-- Outcome token -> the message for a failure that has one of its own. Anything
-- else falls through to the generic `error.actionFailed`, token included.
local FAILURES = {
    ["copy-failed"] = "error.copy-failed",
    ["edit-failed"] = "error.edit-failed",
    ["ocr-failed"] = "error.ocr-failed",
    ["search-failed"] = "error.search-failed",
    ["no-input"] = "error.no-capture",
}

local function fail(body)
    noctalia.notifyError(noctalia.tr("error.title"), body)
end

-- Success notifications are what `notifications_enabled` turns off. Failures and
-- missing tools are always shown, which is the one thing the QML plugin's
-- `--no-notify` also refused to suppress.
local function notifySuccess(title, body)
    if overlay.notificationsEnabled() then
        noctalia.notify(title, body)
    end
end

-- Recording notifications have a switch of their own, on top of the global one —
-- record.sh's `--notify` was passed only when both were on.
local function notifyRecording(title, body)
    if overlay.notificationsEnabled() and overlay.recordingNotifications() then
        noctalia.notify(title, body)
    end
end

-- Run `capture.sh` for one action and hand every outcome token to `onOutcome`.
--
-- A shell script rather than a chain of `runAsync` calls, for two reasons that
-- are both about time: the editor is interactive and tesseract is unbounded,
-- while `runAsync` with a callback kills its child at the 60 s ceiling; and only
-- a shell can hand `wl-copy` a file to read (`< "$1"`) and then delete that file
-- once the clipboard owner has it. `runStream` has no timeout and the script's
-- last line is the report, which is the same trade the overlay launch makes.
local function runScript(action, source, extra, onOutcome)
    local dir = noctalia.pluginDir()
    if type(dir) ~= "string" or dir == "" then
        fail(noctalia.tr("error.no-plugin-dir"))
        return false
    end

    local argv = { "sh", dir .. "/" .. CAPTURE_SCRIPT, "--action", action, "--file", source }
    for _, value in ipairs(extra or {}) do
        table.insert(argv, value)
    end
    for i, value in ipairs(argv) do
        argv[i] = overlay.shellQuote(value)
    end

    -- `false` means the host refused the stream — four are already live, which
    -- is only reachable by starting several edits without saving any. Worth
    -- saying out loud: the click looked like it worked.
    if not noctalia.runStream(table.concat(argv, " "), onOutcome) then
        fail(noctalia.tr("error.not-started"))
        return false
    end
    return true
end

-- Turn one `capture.sh` outcome token into a notification.
--
-- Takes the *target*, not the script's action name: the caller knows which
-- button the user pressed, and the failure message names it ("the Screenshot
-- action failed"), which is the thing the user recognises.
local function outcomeHandler(target, paths)
    return function(raw)
        local outcome = noctalia.string.trim(raw)
        if outcome == "" then
            return
        end

        local missing = outcome:match("^missing:(.*)$")
        if missing ~= nil then
            noctalia.notifyError(noctalia.tr("missing.title"), noctalia.tr("missing.tool", { tool = missing }))
            return
        end

        local known = OUTCOMES[outcome]
        if known ~= nil then
            local body = nil
            if known.body ~= nil then
                body = noctalia.tr(known.body)
            end
            if outcome == "saved" and paths ~= nil then
                body = paths.final
            end
            notifySuccess(noctalia.tr(known.title), body)
            return
        end

        -- The raw token rides along: every failure here is a shell tool that
        -- said no, and the token is the only part of that the user can act on.
        fail(noctalia.tr(FAILURES[outcome] or "error.action-failed", {
            action = noctalia.tr("target." .. target),
            reason = outcome,
        }))
    end
end

local function copyScreenshot(paths)
    runScript("copy", paths.source, nil, outcomeHandler("screenshot", paths))
end

local function editScreenshot(paths)
    local extra = { "--editor", overlay.editor(), "--output", paths.final }
    if overlay.keepSource() then
        table.insert(extra, "--keep-source")
    end
    runScript("edit", paths.source, extra, outcomeHandler("screenshot", paths))
end

-- Start a recording of the region the user just picked.
--
-- Nothing to clean up: a recording target asks the overlay for the region only,
-- so no crop was ever written. QML never captured anything here either; this port
-- has to capture the *screen* to draw its frozen background, but the rectangle is
-- the whole of what a recording needs from it.
local function startRecording(target, report)
    local region = overlay.globalRegion(report)
    if region == nil then
        fail(noctalia.tr("error.no-region"))
        return
    end

    local dir = overlay.recordDir()
    local made, err = noctalia.mkdirAll(dir)
    if not made then
        fail(noctalia.tr("error.no-record-dir", { dir = dir, reason = tostring(err or dir) }))
        return
    end

    local name = "recording_" .. noctalia.formatTime("%Y-%m-%d_%H.%M.%S") .. ".mp4"
    local argv = {
        RECORDER,
        "--pixel-format",
        "yuv420p",
        "-f",
        dir .. "/" .. name,
        "--geometry",
        string.format("%d,%d %dx%d", region.x, region.y, region.w, region.h),
    }
    -- A scaled output is captured at physical size, so record.sh asked for the
    -- video to be scaled back to the logical size the user selected in, rounding
    -- to even dimensions because H.264 wants them. (record.sh also passed `-t`,
    -- which is not a wf-recorder option on any version to hand — this build
    -- answers `invalid option -- 't'` — so it is not ported.)
    if region.scale > 1.01 then
        local w = math.max(2, math.floor(region.w / 2) * 2)
        local h = math.max(2, math.floor(region.h / 2) * 2)
        table.insert(argv, "-F")
        table.insert(argv, string.format("scale=%d:%d:flags=lanczos", w, h))
    end
    if target == "recordsound" then
        table.insert(argv, "--audio")
    end

    -- Detached: `runAsync` without a callback has no timeout, which is the only
    -- shape that works for something meant to run until the user stops it.
    if not noctalia.runAsync(argv) then
        fail(noctalia.tr("error.record-failed"))
        return
    end
    noctalia.state.set("recording", true)
    notifyRecording(noctalia.tr("recording.starting-title"), name)
end

-- Stop the running recording. `pkill` rather than a signal we hold, because the
-- recorder is detached and its pid is nobody's business here — this is what the
-- QML plugin's `pkill wf-recorder` did, with `-x` so it matches the process
-- *name* and not every command line that happens to mention it.
function M.stopRecording()
    noctalia.runAsync({ "pkill", "-x", RECORDER }, function(result)
        noctalia.state.set("recording", false)
        if result ~= nil and result.exitCode == 0 then
            notifyRecording(noctalia.tr("recording.stopped-title"), noctalia.tr("recording.stopped-body"))
        end
    end, 5000)
end

-- Act on the overlay's report.
function M.handle(target, paths, line)
    local report = overlay.decode(line)
    if report == nil then
        fail(noctalia.tr("error.no-report"))
        return
    end

    -- Busy: another overlay already held the lock, and the user's earlier click
    -- is showing it — nothing to say. Cancelled: they backed out.
    if report.result == "busy" or report.result == "cancelled" then
        return
    end
    if report.result == "failed" then
        fail(tostring(report.error or noctalia.tr("error.no-report")))
        return
    end
    if report.result ~= "delivered" then
        fail(noctalia.tr("error.no-report"))
        return
    end

    if target == "screenshot" then
        -- The binary's one decision, and the only one it makes: which button
        -- finished the selection. QML: right is the editor, everything else the
        -- clipboard.
        if report.button == "right" then
            editScreenshot(paths)
        else
            copyScreenshot(paths)
        end
    elseif target == "ocr" then
        runScript("ocr", paths.source, nil, outcomeHandler("ocr", paths))
    elseif target == "search" then
        runScript("search", paths.source, nil, outcomeHandler("search", paths))
    else
        startRecording(target, report)
    end
end

-- Open the overlay for one target and wire the report to `handle`.
function M.capture(target)
    -- Loud when the overlay is not installed at all. This is the one failure a
    -- stream cannot report: if the command does not exist, or dies before
    -- printing, no line ever arrives and the click would otherwise be silent.
    if not noctalia.commandExists(overlay.binary()) then
        noctalia.notifyError(
            noctalia.tr("missing.title"),
            noctalia.tr("missing.tool", { tool = overlay.binary() })
        )
        return false
    end

    -- A target that wants no image does not want a directory to put one in
    -- either, so a missing data dir is only fatal for the targets that do.
    local paths = overlay.paths(target)
    if paths == nil and IMAGE[target] then
        fail(noctalia.tr("error.no-data-dir"))
        return false
    end

    -- A target with no use for the pixels is launched without `-o`, and the
    -- binary is told `--region-only` instead. `overlay.paths` still handed back a
    -- path for the others.
    local source = IMAGE[target] and paths and paths.source or nil

    -- One report per launch: the first non-blank line is it, and a stray second
    -- line must not run the action again.
    local done = false
    local ok, err = overlay.launch(target, GLYPH[target], source, function(line)
        if done then
            return
        end
        if type(line) ~= "string" or noctalia.string.trim(line) == "" then
            return
        end
        done = true
        M.handle(target, paths, line)
    end)
    if not ok then
        fail(noctalia.tr("error.launch", { reason = tostring(err or overlay.binary()) }))
        return false
    end
    return true
end

-- The entry point both entries use for a target.
function M.run(target)
    -- `GLYPH`, not `TARGETS`: the list above is an *array*, so `TARGETS[target]`
    -- indexes it by position — `TARGETS["screenshot"]` is nil and every id,
    -- including the widget's own, was refused as unknown. `GLYPH` is the same
    -- table keyed by id, which is the lookup this guard actually wants.
    if not GLYPH[target] then
        fail(noctalia.tr("error.unknown-target", { target = tostring(target) }))
        return false
    end

    -- Recording toggles instead of opening the overlay: the QML plugin checked
    -- `pidof wf-recorder` before showing a selector, so asking to record while a
    -- recording is live means stop.
    if target == "record" or target == "recordsound" then
        noctalia.processMatches(function(running)
            if running then
                M.stopRecording()
            else
                M.capture(target)
            end
        end, RECORDER)
        return true
    end

    for _, tool in ipairs(TARGET_TOOLS[target] or {}) do
        if not noctalia.commandExists(tool) then
            noctalia.notifyError(noctalia.tr("missing.title"), noctalia.tr("missing.tool", { tool = tool }))
            return false
        end
    end

    return M.capture(target)
end

return M
