--!nonstrict
-- 显示 SPlayer-Next 当前歌词的状态栏组件。
--
-- 轮询 bridge.py 的 /state，只保留当前行；bridge 负责接收 SPlayer-Next 的推送。

local function clampInt(value, fallback, lower, upper)
  local number = tonumber(value)
  if number == nil then
    number = fallback
  end
  number = math.floor(number)
  if number < lower then
    return lower
  end
  if number > upper then
    return upper
  end
  return number
end

local HOST = noctalia.getConfig("host") or "127.0.0.1"
HOST = noctalia.string.trim(tostring(HOST))
HOST = string.gsub(HOST, "^https?://", "")
HOST = string.gsub(HOST, "/.*$", "")
if HOST == "" then
  HOST = "127.0.0.1"
end

local function validPort(value)
  local number = tonumber(value)
  if number == nil then
    return 33550
  end
  number = math.floor(number)
  if number < 1024 or number > 65535 then
    return 33550
  end
  return number
end

local PORT = validPort(noctalia.getConfig("port"))

local PREFIX = tostring(noctalia.getConfig("prefix") or "")
local MAX_CHARS = clampInt(noctalia.getConfig("max_chars"), 40, 0, 300)
local POLL_MS = clampInt(noctalia.getConfig("poll_ms"), 300, 100, 5000)
local EXTRA_MODE = tostring(noctalia.getConfig("extra_mode") or "background-duet")
local EXTRA_MODES = {
  ["none"] = true,
  ["background-duet"] = true,
  ["duet-translation"] = true,
  ["translation-fallback-next"] = true,
  ["translation"] = true,
  ["next"] = true,
}
if not EXTRA_MODES[EXTRA_MODE] then
  EXTRA_MODE = "background-duet"
end
local HIDE_WHEN_IDLE = noctalia.getConfig("hide_when_idle") ~= false
local SHOW_WHEN_PAUSED = noctalia.getConfig("show_when_paused") == true
local SHOW_TITLE_WHEN_NO_LYRIC = noctalia.getConfig("show_title_when_no_lyric") ~= false

local IDLE_TEXT = "—"

local serverHost = HOST
if string.find(serverHost, ":", 1, true) and string.sub(serverHost, 1, 1) ~= "[" then
  serverHost = "[" .. serverHost .. "]"
end
local STATE_URL = "http://" .. serverHost .. ":" .. PORT .. "/state"

local inFlight = false
local intervalSet = false
local online = false
local lastAttemptSec = 0
local visible = true
local lastKey = nil
local lastState = nil

local function trim(value)
  if type(value) ~= "string" then
    return ""
  end
  return noctalia.string.trim(value)
end

local function truncate(text, limit)
  if limit <= 0 then
    return text
  end
  local characters = utf8.len(text)
  if characters == nil then
    if #text <= limit then
      return text
    end
    return string.sub(text, 1, limit) .. "…"
  end
  if characters <= limit then
    return text
  end
  local offset = utf8.offset(text, limit + 1)
  if offset == nil then
    return text
  end
  return string.sub(text, 1, offset - 1) .. "…"
end

local function fit(text)
  if MAX_CHARS <= 0 then
    return PREFIX .. text
  end
  local prefixCharacters = utf8.len(PREFIX) or #PREFIX
  local room = MAX_CHARS - prefixCharacters
  if room < 1 then
    room = 1
  end
  return PREFIX .. truncate(text, room)
end

local function setVisible(next)
  if visible == next then
    return
  end
  visible = next
  barWidget.setVisible(next)
end

local function hide()
  setVisible(false)
  lastKey = nil
end

-- 空闲状态统一收口：隐藏，或显示「—」。
local function renderIdle(glyph)
  if HIDE_WHEN_IDLE then
    hide()
    return
  end
  setVisible(true)
  local key = "idle|" .. glyph .. "|" .. IDLE_TEXT
  if key == lastKey then
    return
  end
  lastKey = key
  barWidget.setGlyph(glyph)
  barWidget.setGlyphColor("on_surface")
  barWidget.setText(fit(IDLE_TEXT))
end

local function render(data)
  if type(data) ~= "table" or data.stale == true or data.hasState ~= true then
    renderIdle("music-off")
    return
  end

  local title = trim(data.title)
  local lyric = ""
  local extra = ""
  local extraKind = ""
  local playing = data.playing == true

  if data.protocol == 2 then
    local current = trim(data.current)
    local currentTranslation = trim(data.currentTranslation)
    local bgMain = trim(data.bgMain)
    local bgMainTranslation = trim(data.bgMainTranslation)
    local duetMain = trim(data.duetMain)
    local duetMainTranslation = trim(data.duetMainTranslation)
    local background = trim(data.background)
    local duet = trim(data.duet)
    local nextLine = trim(data.next)
    local isBG = data.isBG == true
    local isDuet = data.isDuet == true
    local duetOverlap = data.duetOverlap == true

    local mainText = current
    local mainTranslation = currentTranslation
    if isBG then
      if bgMain ~= "" then
        mainText = bgMain
        mainTranslation = bgMainTranslation
      end
    elseif isDuet and duetOverlap and EXTRA_MODE == "background-duet" then
      if duetMain ~= "" then
        mainText = duetMain
        mainTranslation = duetMainTranslation
      end
    end

    if EXTRA_MODE == "background-duet" then
      if isBG then
        extra = background
        extraKind = "bg"
      elseif isDuet and duetOverlap then
        if background ~= "" then
          extra = background
          extraKind = "bg"
        elseif duet ~= "" then
          extra = duet
          extraKind = "duet"
        end
      elseif background ~= "" then
        extra = background
        extraKind = "bg"
      end
      if extra == "" then
        if mainTranslation ~= "" then
          extra = mainTranslation
          extraKind = "translation"
        elseif nextLine ~= "" then
          extra = nextLine
          extraKind = "next"
        end
      end
    elseif EXTRA_MODE == "duet-translation" then
      if isDuet and duetOverlap then
        mainText = current
        mainTranslation = currentTranslation
      end
      if mainTranslation ~= "" then
        extra = mainTranslation
        extraKind = "translation"
      elseif nextLine ~= "" then
        extra = nextLine
        extraKind = "next"
      end
    elseif EXTRA_MODE == "translation-fallback-next" then
      if mainTranslation ~= "" then
        extra = mainTranslation
        extraKind = "translation"
      elseif nextLine ~= "" then
        extra = nextLine
        extraKind = "next"
      end
    elseif EXTRA_MODE == "translation" then
      if mainTranslation ~= "" then
        extra = mainTranslation
        extraKind = "translation"
      end
    elseif EXTRA_MODE == "next" then
      if nextLine ~= "" then
        extra = nextLine
        extraKind = "next"
      end
    end

    lyric = mainText
  else
    lyric = trim(data.lyric)
    extra = trim(data.extra)
    extraKind = trim(data.extraKind)
  end

  if not playing and not SHOW_WHEN_PAUSED then
    renderIdle("pause")
    return
  end

  local content = lyric
  if content == "" and SHOW_TITLE_WHEN_NO_LYRIC then
    content = title
  end
  if content == "" then
    renderIdle("pause")
    return
  end

  if extra ~= "" and lyric ~= "" then
    local separator = extraKind == "next" and "  →  " or "  ·  "
    content = content .. separator .. extra
  end

  local key = table.concat({
    tostring(playing),
    title,
    lyric,
    extra,
    extraKind,
    EXTRA_MODE,
    tostring(MAX_CHARS),
    PREFIX,
  }, "\31")

  setVisible(true)
  if key == lastKey then
    return
  end
  lastKey = key

  barWidget.setGlyph(playing and "disc" or "pause")
  barWidget.setGlyphColor(playing and "primary" or "on_surface")
  barWidget.setText(fit(content))
end

local function poll()
  if inFlight then
    return
  end
  -- 离线时退到每 2 秒重试，避免按 poll_ms 反复连接。
  local nowSec = os.time()
  if not online and nowSec - lastAttemptSec < 2 then
    return
  end
  lastAttemptSec = nowSec
  inFlight = true
  local accepted = noctalia.http({
    url = STATE_URL,
    headers = { "Accept: application/json" },
  }, function(response)
    inFlight = false
    if response == nil or response.ok ~= true or response.status ~= 200 then
      online = false
      lastState = nil
      render(nil)
      return
    end
    local data = noctalia.json.decode(response.body)
    if type(data) ~= "table" then
      online = false
      lastState = nil
      render(nil)
      return
    end
    online = true
    lastState = data
    render(data)
  end)
  if not accepted then
    inFlight = false
  end
end

function update()
  if not intervalSet then
    intervalSet = true
    noctalia.setUpdateInterval(POLL_MS)
  end
  poll()
end

function onIpc(event)
  if event == "status" then
    if lastState == nil then
      noctalia.log("splayer-lyrics: no state yet")
    else
      noctalia.log("splayer-lyrics: " .. (noctalia.json.encode(lastState) or "{}"))
    end
    return
  end
  noctalia.log("splayer-lyrics: unknown ipc event " .. tostring(event))
end
