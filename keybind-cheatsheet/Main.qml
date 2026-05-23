import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons

Item {
  id: root
  property var pluginApi: null

  // ── Lifecycle ──

  property bool parserStarted: false
  property int cheatsheetDataVersion: 0

  Component.onCompleted: {
    if (pluginApi && !parserStarted) {
      checkAndParse();
    }
  }

  onPluginApiChanged: {
    if (pluginApi && !parserStarted) {
      checkAndParse();
    }
  }

  Component.onDestruction: {
    if (hyprctlBindsProcess.running) hyprctlBindsProcess.running = false;
  }

  function checkAndParse() {
    var hasData = (pluginApi?.pluginSettings?.cheatsheetData || []).length > 0;
    if (!hasData) {
      parserStarted = true;
      runParser();
    } else {
      parserStarted = true;
    }
  }

  function refresh() {
    if (!pluginApi) return;
    parserStarted = false;
    parserStarted = true;
    runParser();
  }

  // ── Parser: hyprctl binds -j ──

  property var hyprctlChunks: []

  function runParser() {
    hyprctlChunks = [];
    hyprctlBindsProcess.command = ["hyprctl", "binds", "-j"];
    hyprctlBindsProcess.running = true;
  }

  Process {
    id: hyprctlBindsProcess
    running: false

    stdout: SplitParser {
      onRead: data => {
        if (root.hyprctlChunks.length < 20000) root.hyprctlChunks.push(data);
      }
    }

    onExited: (exitCode, exitStatus) => {
      if (exitCode !== 0 || root.hyprctlChunks.length === 0) {
        Logger.e("keybind-cheatsheet", "hyprctl binds -j failed (exit " + exitCode + ")");
        root.hyprctlChunks = [];
        root.saveToDb([{
          "title": root.pluginApi?.tr("error.unsupported-compositor"),
          "binds": [{ "keys": "hyprctl", "desc": root.pluginApi?.tr("error.hyprctl-failed") }]
        }]);
        return;
      }
      var binds = [];
      try {
        binds = JSON.parse(root.hyprctlChunks.join("\n"));
      } catch (e) {
        Logger.e("keybind-cheatsheet", "hyprctl JSON parse failed: " + e);
        binds = [];
      }
      root.hyprctlChunks = [];
      root.buildCategories(binds);
    }
  }

  // ── Description tags ──
  // Format: [类别 N] 内容  or  [类别 hidden] 内容
  function parseDescription(desc) {
    if (!desc) return { category: null, priority: 9999, hidden: false, content: "" };
    var m = desc.match(/^\s*\[([^\]]+)\]\s*(.*)$/);
    if (!m) return { category: null, priority: 9999, hidden: false, content: desc.trim() };
    var tag = m[1].trim();
    var content = m[2].trim();
    if (/\bhidden\b/i.test(tag)) return { category: null, priority: 9999, hidden: true, content: content };
    var parts = tag.split(/\s+/);
    var priority = 9999;
    var category = tag;
    if (parts.length >= 2) {
      var n = parseInt(parts[parts.length - 1], 10);
      if (!isNaN(n)) {
        priority = n;
        category = parts.slice(0, -1).join(" ");
      }
    }
    return { category: category, priority: priority, hidden: false, content: content };
  }

  // ── hyprctl helpers ──

  function decodeModmask(mask) {
    var m = [];
    if (mask & 64) m.push("Super");
    if (mask & 1)  m.push("Shift");
    if (mask & 4)  m.push("Ctrl");
    if (mask & 8)  m.push("Alt");
    if (mask & 16) m.push("Mod2");
    if (mask & 32) m.push("Mod3");
    if (mask & 128) m.push("Mod5");
    return m;
  }

  function formatSpecialKey(key) {
    var keyMap = {
      "XF86AUDIORAISEVOLUME": "Vol Up",
      "XF86AUDIOLOWERVOLUME": "Vol Down",
      "XF86AUDIOMUTE": "Mute",
      "XF86AUDIOMICMUTE": "Mic Mute",
      "XF86AUDIOPLAY": "Play",
      "XF86AUDIOPAUSE": "Pause",
      "XF86AUDIONEXT": "Next",
      "XF86AUDIOPREV": "Prev",
      "XF86AUDIOSTOP": "Stop",
      "XF86AUDIOMEDIA": "Media",
      "XF86MONBRIGHTNESSUP": "Bright Up",
      "XF86MONBRIGHTNESSDOWN": "Bright Down",
      "XF86CALCULATOR": "Calc",
      "XF86MAIL": "Mail",
      "XF86SEARCH": "Search",
      "XF86EXPLORER": "Files",
      "XF86WWW": "Browser",
      "XF86HOMEPAGE": "Home",
      "XF86FAVORITES": "Favorites",
      "XF86POWEROFF": "Power",
      "XF86SLEEP": "Sleep",
      "XF86EJECT": "Eject",
      "PRINT": "PrtSc",
      "Print": "PrtSc",
      "PRIOR": "PgUp",
      "NEXT": "PgDn",
      "Prior": "PgUp",
      "Next": "PgDn",
      "MOUSE_DOWN": "Scroll Down",
      "MOUSE_UP": "Scroll Up",
      "MOUSE:272": "Left Click",
      "MOUSE:273": "Right Click",
      "MOUSE:274": "Middle Click"
    };
    return keyMap[key] || key;
  }

  // ── Category builder ──

  readonly property var keyBlacklist: ["Super_L", "Super_R"]

  function buildCategories(binds) {
    var showUndescribed = pluginApi?.pluginSettings?.showUndescribedBinds ?? true;
    var otherTitle = pluginApi?.tr("panel.other") || "Other";
    var undescTitle = pluginApi?.tr("panel.undescribed") || "Undescribed";

    var byCat = ({});
    var catPriority = ({});

    for (var i = 0; i < binds.length; i++) {
      var b = binds[i];
      if (!b || b.key === undefined) continue;

      var rawDesc = (b.has_description && b.description) ? b.description : "";
      var undescribed = (rawDesc === "");

      if (undescribed && !showUndescribed) continue;

      var parsed = parseDescription(rawDesc);
      if (parsed.hidden) continue;

      var keyName = formatSpecialKey(b.key);
      var mods = decodeModmask(b.modmask);

      var fullKey = "";

      if (mods.length === 1 && mods[0] === "Super" && keyBlacklist.includes(keyName)) {
        fullKey = "Super";
      } else {
        fullKey = mods.length > 0 ? (mods.join(" + ") + " + " + keyName) : keyName;
      }

      var catName = undescribed ? undescTitle : (parsed.category || otherTitle);

      if (!byCat[catName]) {
        byCat[catName] = [];
        catPriority[catName] = parsed.priority;
      } else if (parsed.priority < catPriority[catName]) {
        catPriority[catName] = parsed.priority;
      }

      byCat[catName].push({
        "keys": fullKey,
        "desc": undescribed ? "" : parsed.content,
        "undescribed": undescribed,
        "_priority": parsed.priority
      });
    }

    for (var k in byCat) {
      byCat[k].sort(function(a, b) { return a._priority - b._priority; });
    }

    var sortedKeys = Object.keys(byCat).sort(function(a, b) {
      if (catPriority[a] !== catPriority[b]) return catPriority[a] - catPriority[b];
      return a.localeCompare(b);
    });

    var categories = [];
    for (var j = 0; j < sortedKeys.length; j++) {
      categories.push({ "title": sortedKeys[j], "binds": byCat[sortedKeys[j]] });
    }

    saveToDb(categories);
  }

  // ── Save to plugin settings ──

  function saveToDb(data) {
    if (pluginApi) {
      pluginApi.pluginSettings.cheatsheetData = data;
      pluginApi.pluginSettings.detectedCompositor = "hyprland";
      pluginApi.saveSettings();
      cheatsheetDataVersion++;
    }
  }

  // ── IPC handler ──

  IpcHandler {
    target: "plugin:keybind-cheatsheet"

    function toggle() {
      if (root.pluginApi) {
        root.pluginApi.withCurrentScreen(screen => {
          root.pluginApi.togglePanel(screen);
        });
      }
    }

    function refresh() {
      root.refresh();
    }
  }
}
