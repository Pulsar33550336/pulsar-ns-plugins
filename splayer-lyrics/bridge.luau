--!nonstrict
-- 管理 bridge.py 的启动与停止。
--
-- 插件启用时用 setsid 拉起桥，停用或 Noctalia 退出时停止；桥的 --owner-pid
-- 看门狗会在 Noctalia 消失后自行退出，避免留下孤儿进程。
-- setsid -f 已脱离会话，runAsync 也立即返回，不需要再 disown。

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

local function readHost()
  local host = noctalia.getConfig("host") or "127.0.0.1"
  host = noctalia.string.trim(tostring(host))
  host = string.gsub(host, "^https?://", "")
  host = string.gsub(host, "/.*$", "")
  if host == "" then
    host = "127.0.0.1"
  end
  return host
end

local HOST = readHost()
local PORT = validPort(noctalia.getConfig("port"))

local PLUGIN_DIR = noctalia.pluginDir() or ""
local BRIDGE = PLUGIN_DIR .. "/bridge.py"
local OWNER_PID = 0

local function readOwnerPid()
  -- 插件 VM 跑在 Noctalia 进程里，/proc/self 就是 Noctalia。
  local stat = noctalia.readFile("/proc/self/stat")
  if type(stat) ~= "string" then
    return 0
  end
  return tonumber(string.match(stat, "^(%d+)")) or 0
end
OWNER_PID = readOwnerPid()

local function urlHost(host)
  if host == "0.0.0.0" or host == "::" or host == "[::]" then
    return "127.0.0.1"
  end
  if string.find(host, ":", 1, true) and string.sub(host, 1, 1) ~= "[" then
    return "[" .. host .. "]"
  end
  return host
end

local HEALTH_URL = "http://" .. urlHost(HOST) .. ":" .. PORT .. "/health"

local function shQuote(value)
  return "'" .. string.gsub(value, "'", "'\\''") .. "'"
end

local function runDetached(program, args)
  local parts = {}
  if noctalia.commandExists("setsid") then
    parts[#parts + 1] = "setsid -f"
  end
  parts[#parts + 1] = "nohup"
  parts[#parts + 1] = shQuote(program)
  for _, argument in ipairs(args) do
    parts[#parts + 1] = shQuote(argument)
  end
  parts[#parts + 1] = ">/dev/null 2>&1"
  return noctalia.runAsync(table.concat(parts, " "))
end

-- onExit 返回后 VM 会被销毁，不能依赖 noctalia.http 回调；用 detached 脚本
-- 请求 /quit，并重试几次，避免禁用插件后桥继续运行。
local function buildQuitCode(host, port)
  return table.concat({
    "import time, urllib.request",
    "url = \"http://" .. urlHost(host) .. ":" .. port .. "/quit\"",
    "for _ in range(5):",
    "    try:",
    "        urllib.request.urlopen(url, data=b'', timeout=1)",
    "        break",
    "    except Exception:",
    "        time.sleep(0.4)",
  }, "\n")
end

local QUIT_CODE = buildQuitCode(HOST, PORT)

local function quit()
  runDetached("python3", { "-c", QUIT_CODE })
end

local intervalSet = false
local launched = false
local lastLaunch = 0
local checking = false
local warnedMissing = false

local function launch()
  if BRIDGE == "" or not noctalia.fileExists(BRIDGE) then
    if not warnedMissing then
      warnedMissing = true
      noctalia.log("splayer-lyrics: bridge.py not found at " .. tostring(BRIDGE))
    end
    return
  end
  if not noctalia.commandExists("python3") then
    if not warnedMissing then
      warnedMissing = true
      noctalia.log("splayer-lyrics: python3 not found on PATH")
    end
    return
  end

  local now = os.time()
  if now - lastLaunch < 3 then
    return
  end
  lastLaunch = now

  local ok = runDetached("python3", {
    BRIDGE,
    "--host",
    HOST,
    "--port",
    tostring(PORT),
    "--owner-pid",
    tostring(OWNER_PID),
    "--bind-retries",
    "20",
    "--bind-retry-delay",
    "0.5",
  })
  launched = ok
  if not ok then
    noctalia.log("splayer-lyrics: failed to launch bridge.py")
  end
end

local function checkHealth()
  if checking then
    return
  end
  checking = true
  local accepted = noctalia.http({ url = HEALTH_URL }, function(response)
    checking = false
    if response ~= nil and response.ok == true and response.status == 200 then
      launched = true
      return
    end
    launched = false
    launch()
  end)
  if not accepted then
    checking = false
  end
end

function update()
  if not intervalSet then
    intervalSet = true
    noctalia.setUpdateInterval(5000)
    -- 每次 service 加载都重新拉起一个桥；旧实例在 onExit 里收到 /quit，
    -- 新实例用 --bind-retries 等端口释放。
    launch()
  end
  checkHealth()
end

function onConfigChanged()
  local nextHost = readHost()
  local nextPort = validPort(noctalia.getConfig("port"))
  if nextHost == HOST and nextPort == PORT then
    return
  end
  -- 先按旧端口停桥，再换到新端口重新拉起；否则旧桥会一直占着旧端口。
  quit()
  HOST = nextHost
  PORT = nextPort
  HEALTH_URL = "http://" .. urlHost(HOST) .. ":" .. PORT .. "/health"
  QUIT_CODE = buildQuitCode(HOST, PORT)
  launched = false
  lastLaunch = 0
  launch()
end

function onExit(_signal, _reason)
  -- reload 也停：新 runtime 会重新 launch，这样端口/脚本版本都不会残留。
  quit()
end

function onIpc(event)
  if event == "restart" then
    quit()
    lastLaunch = 0
    launch()
    return
  end
  if event == "status" then
    noctalia.log(
      string.format(
        "splayer-lyrics: bridge=%s host=%s port=%d owner=%d launched=%s",
        BRIDGE,
        HOST,
        PORT,
        OWNER_PID,
        tostring(launched)
      )
    )
    return
  end
  noctalia.log("splayer-lyrics: unknown ipc event " .. tostring(event))
end
