--!nonstrict
-- Everything that talks to `screenshot-overlay`.
--
-- The split this module exists to hold: the plugin resolves, the binary draws.
-- Nothing here knows what Noctalia's palette file is called or where it lives —
-- `noctalia.getColor(role)` hands back the shell's live palette and this maps
-- the roles onto the binary's flags. Nothing here knows what a "target" *means*
-- either; it knows how to start the overlay for one and how to read the report
-- it prints. What the report means is `actions.luau`.
--
-- Settings come from `noctalia.getConfig`, i.e. the typed schema declared in
-- `plugin.toml`. Every key read here is declared there; the shell's lint tool
-- checks both directions.

local M = {}

local BINARY = "screenshot-overlay"

-- Palette role -> the binary's flag. Roles are the shell's own snake_case tokens
-- (`kColorRoleTokens` in src/ui/palette.h); the mapping is by name, so a role the
-- shell does not know contributes no flag instead of a wrong colour.
local COLOR_ARGS = {
    { "primary", "--primary" },
    { "on_primary", "--on-primary" },
    { "surface", "--surface" },
    { "surface_container", "--surface-container" },
    { "on_surface", "--on-surface" },
    { "outline", "--outline" },
    { "hover", "--hover" },
    { "on_hover", "--on-hover" },
}

-- Every setting is read with a **literal** key below — `noctalia.getConfig("x")`,
-- never through a variable or a table. The shell's lint tool matches those
-- literals against the manifest in both directions, and a computed key would
-- quietly opt this file out of both checks (it counts one dynamic read and then
-- stops asking whether the declared keys are read at all). The helpers here only
-- shape the value that came back.

-- The declared default, repeated as a fallback. `getConfig` returns the seeded
-- default for a declared key, so this only fires if the manifest and this file
-- ever disagree — and a `nil` that silently turned a default-on setting off (or
-- a colour black) is worse than a wrong constant.
local function asBool(value, fallback)
    if type(value) == "boolean" then
        return value
    end
    return fallback
end

-- What the caller needs to know about three of the settings.
function M.notificationsEnabled()
    return asBool(noctalia.getConfig("notifications_enabled"), true)
end

function M.recordingNotifications()
    return asBool(noctalia.getConfig("recording_notifications"), true)
end

function M.keepSource()
    return asBool(noctalia.getConfig("keep_source_screenshot"), false)
end

function M.editor()
    local configured = noctalia.getConfig("screenshot_editor")
    if type(configured) ~= "string" or configured == "" then
        return "swappy"
    end
    return configured
end

function M.binary()
    return BINARY
end

-- `~` expands to HOME; a relative setting is read as relative to HOME rather
-- than to whatever directory the shell happened to be in.
local function absolute(path, fallback)
    local value = path
    if type(value) ~= "string" or value == "" then
        value = fallback
    end
    local expanded = noctalia.expandPath(value)
    if expanded:sub(1, 1) ~= "/" then
        local home = noctalia.getenv("HOME")
        if type(home) == "string" and home ~= "" then
            expanded = home .. "/" .. expanded
        end
    end
    return expanded
end

function M.saveDir()
    return absolute(noctalia.getConfig("save_path"), "~/Pictures/Screenshots")
end

function M.recordDir()
    return absolute(noctalia.getConfig("recording_save_path"), "~/Videos")
end

-- The connector name, sanitised the way the binary sanitises it for its own
-- default filename (`[^A-Za-z0-9_-]` -> `_`).
local function safeOutput()
    local name = noctalia.focusedOutputName()
    if type(name) ~= "string" or name == "" then
        name = "unknown"
    end
    return (name:gsub("[^A-Za-z0-9_-]", "_"))
end

-- Where the binary should write, and — for the editor — where the finished image
-- goes. The names are the QML plugin's: `screenshot_<stamp>_<output>.png`, with
-- the editor's input as `..._source.png`.
--
-- The screenshot target writes into the save folder, because
-- `keep_source_screenshot` is about keeping a file the user can find afterwards.
-- Every other target writes a scratch file into the plugin's data directory,
-- because there the PNG is only an argument to the real action.
function M.paths(target)
    if target == "screenshot" then
        local dir = M.saveDir()
        local stem = "screenshot_" .. noctalia.formatTime("%Y-%m-%d_%H.%M.%S") .. "_" .. safeOutput()
        return {
            source = dir .. "/" .. stem .. "_source.png",
            final = dir .. "/" .. stem .. ".png",
        }
    end

    local dir = noctalia.pluginDataDir()
    if type(dir) ~= "string" or dir == "" then
        return nil
    end
    return { source = dir .. "/capture-" .. target .. ".png" }
end

-- Quote one argument for the shell. `runStream` runs its command through
-- `/bin/sh -lc`, so a bare `#rrggbb` would start a comment and the colour would
-- vanish — that is not hypothetical, `--primary #a9c7ff` unquoted fails with
-- "a value is required" and exit 2.
function M.shellQuote(value)
    return "'" .. tostring(value):gsub("'", "'\\''") .. "'"
end

-- The theme's colours as binary arguments. A role the shell cannot resolve
-- yields no flag: the binary's own defaults are the stock Noctalia palette, so
-- launching anyway beats refusing to take a screenshot.
local function themeArgs()
    local args = {}
    for _, pair in ipairs(COLOR_ARGS) do
        local value = noctalia.getColor(pair[1])
        -- getColor already returns canonical #RRGGBB or nil, but guard the shape
        -- anyway: a malformed value would be rejected by the binary's parser and
        -- fail the whole launch.
        if type(value) == "string" and value:match("^#%x%x%x%x%x%x$") then
            table.insert(args, pair[2])
            table.insert(args, value)
        end
    end
    return args
end

-- The settings that change how the overlay itself behaves.
local function overlayArgs()
    local args = {}
    -- `== false` rather than a falsy test: a declared bool is always a boolean,
    -- and a `nil` from a manifest this file has outgrown must not flip a
    -- default-on setting off.
    if noctalia.getConfig("enable_cross") == false then
        table.insert(args, "--cursor")
        table.insert(args, "default")
    end
    if noctalia.getConfig("enable_windows_selection") == false then
        table.insert(args, "--no-window-detection")
    end
    local level = noctalia.getConfig("png_compression_level")
    if type(level) ~= "number" then
        level = 1
    end
    table.insert(args, "--png-compression")
    table.insert(args, tostring(math.floor(level)))

    -- The bar's controls mimic the shell's, and `shell.corner_radius_scale` is
    -- the one style value a user can turn (Settings' "corner roundness"). It is
    -- read here rather than assumed, because a bar with the shell's default
    -- corners next to a shell the user squared off would look like a bug. Every
    -- other metric the bar uses is a compile-time constant in the shell, so there
    -- is nothing else to ask for.
    --
    -- `getSetting`, not `getConfig`: this is the *shell's* config, not one of
    -- this plugin's declared settings. Same call, same layer, as `getColor` —
    -- neither reads a config file itself.
    local radius = noctalia.getSetting("shell.corner_radius_scale")
    if type(radius) == "number" then
        table.insert(args, "--corner-radius-scale")
        table.insert(args, tostring(radius))
    end
    return args
end

-- Start the overlay. Returns `false, reason` when the host refused the stream
-- (no worker, or four already live), which is the only failure it reports.
--
-- Both pieces of the bar's presentation are arguments: `glyph` for its icon and
-- `target` only to pick the translated label. The binary knows no target at all —
-- which icon and which words stand for "ocr" is `lib/actions.luau`'s table, and
-- it is passed in rather than duplicated here.
--
-- `source` is where the image should go, or `nil` for a caller that wants only
-- the rectangle: then the binary is told `--region-only`, so it never crops or
-- encodes anything and there is no path to pass and no directory to make.
function M.launch(target, glyph, source, onReport)
    local argv = {
        BINARY,
        -- The Tabler name, resolved by the binary against its own vendored copy
        -- of the table. A name that is not in it is refused at parse time, so a
        -- typo here fails loudly instead of drawing an empty box.
        "--target-icon",
        glyph,
        -- The overlay's own bar says what this selection is for; the label is
        -- the same translated string the panel button showed.
        "--label",
        noctalia.tr("target." .. target),
    }

    if source == nil then
        table.insert(argv, "--region-only")
    else
        -- The binary refuses to create an explicit `-o`'s parent on purpose — a
        -- mistyped path should fail rather than grow a tree — so making the
        -- folder is the caller's job. `capture.sh` did this too
        -- (`mkdir -p "$(dirname ...)"`).
        local dir = source:match("^(.*)/[^/]+$")
        if dir ~= nil then
            local ok, err = noctalia.mkdirAll(dir)
            if not ok then
                return false, tostring(err or dir)
            end
        end
        table.insert(argv, "-o")
        table.insert(argv, source)
    end

    for _, value in ipairs(overlayArgs()) do
        table.insert(argv, value)
    end
    for _, value in ipairs(themeArgs()) do
        table.insert(argv, value)
    end
    for i, value in ipairs(argv) do
        argv[i] = M.shellQuote(value)
    end

    -- `runStream`, not `runAsync`. `runAsync` *with a callback* runs its child
    -- under a deadline (`kDefaultCommandTimeout` 5 s, `kMaxCommandTimeout` 60 s)
    -- and kills its whole process group when that expires — fatal here, because
    -- the overlay is interactive and the user decides how long a selection takes.
    -- Streams are started without a timeout, and the report still arrives: the
    -- binary prints it as its only stdout line.
    return noctalia.runStream(table.concat(argv, " "), onReport)
end

-- The report: one bare JSON object, the binary's only stdout line. Returns nil
-- for a blank or unparseable line — the caller decides how loud to be about it.
function M.decode(line)
    if type(line) ~= "string" then
        return nil
    end
    local trimmed = noctalia.string.trim(line)
    if trimmed == "" then
        return nil
    end
    local report = noctalia.json.decode(trimmed)
    if type(report) ~= "table" then
        return nil
    end
    return report
end

-- The selection turned into a compositor geometry.
--
-- The report carries the rectangle in output-local **logical** pixels, which is
-- the space `wf-recorder --geometry` reads; a global geometry is that plus the
-- output's own origin. `noctalia.outputs()` is where the origin and the scale
-- come from, which is the whole reason the region is in the report at all —
-- the binary knows the output, the plugin knows where the compositor put it.
--
-- Returns nil when the report has no usable rectangle.
function M.globalRegion(report)
    local region = report.region
    if type(region) ~= "table" then
        return nil
    end
    local x, y = tonumber(region.x), tonumber(region.y)
    local w, h = tonumber(region.w), tonumber(region.h)
    if x == nil or y == nil or w == nil or h == nil or w <= 0 or h <= 0 then
        return nil
    end

    local originX, originY, scale = 0, 0, 1
    local monitor = report.monitor
    if type(monitor) == "string" then
        for _, output in ipairs(noctalia.outputs()) do
            if output.name == monitor then
                originX = tonumber(output.x) or 0
                originY = tonumber(output.y) or 0
                -- `outputs()` reports the scale as an integer, so anything above
                -- 1 here is a genuinely scaled output.
                scale = tonumber(output.scale) or 1
                break
            end
        end
    end

    return {
        x = math.floor(x + 0.5) + originX,
        y = math.floor(y + 0.5) + originY,
        w = w,
        h = h,
        scale = scale,
    }
end

return M
