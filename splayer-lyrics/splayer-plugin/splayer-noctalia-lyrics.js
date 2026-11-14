/**
 * @name Noctalia 歌词桥
 * @id pulsar.splayer-noctalia-lyrics
 * @version 1.0.0
 * @author Pulsar
 * @description 把 SPlayer-Next 当前歌词推送到本机 Noctalia 状态栏（配合 bridge.py）
 * @apiLevel 2
 * @grant network
 * @type control
 *
 * SPlayer-Next 1.1.0 只向 manifest.type == "control" 的插件广播播放事件；
 * 少了 @type control，插件只会跑心跳、收不到 trackChange/lineChange。
 */
const DEFAULT_CREDIT_KEYWORDS = [
  "作词", "作曲", "编曲", "编曲制作", "词曲", "词", "曲",
  "制作人", "制作", "监制", "出品", "发行", "制谱", "统筹", "策划",
  "混音", "混音师", "母带", "母带师", "录音", "录音师", "配唱",
  "和声", "和音", "合声", "人声", "演唱", "原唱",
  "吉他", "贝斯", "鼓", "键盘", "钢琴", "弦乐", "乐队", "乐团", "小提琴", "中提琴",
  "大提琴", "弦乐四重奏", "四重奏", "萨克斯", "小号", "长号", "口琴", "笛", "琵琶", "古筝", "二胡",
  "封面", "设计", "翻译", "校对",
  "Music", "Lyrics", "Lyric", "Composer", "Arranger", "Producer",
  "Mixing", "Mix", "Mastering", "Master", "Guitar", "Bass", "Drums",
  "Piano", "Violin", "Viola", "Cello", "Orchestra", "String Quartet", "Quartet", "Sax", "Saxophone", "Trumpet",
  "Trombone", "Vocals", "Vocal", "Recorded", "Recording", "Studio",
  "Engineer", "Written", "Composed", "Arranged", "Produced", "Performed",
  "Lyricist", "Songwriter", "Coordinator", "Design", "Cover", "Art",
  "Translation", "OP", "SP", "A&R",
].join(",");

splayer.register({
  events: ["trackChange", "lyricChange", "lineChange", "playStateChange"],
  settings: [
    {
      key: "host",
      type: "text",
      label: "桥地址 (Bridge host)",
      description: "bridge.py 的监听地址，通常保持 127.0.0.1",
      default: "127.0.0.1",
      placeholder: "127.0.0.1",
    },
    {
      key: "port",
      type: "number",
      label: "桥端口 (Bridge port)",
      description: "必须与 bridge.py 以及 Noctalia 组件里的端口一致",
      default: 33550,
      min: 1024,
      max: 65535,
    },
    {
      key: "filterCredits",
      type: "switch",
      label: "过滤制作人员行",
      description: "跳过作词/作曲/编曲/演奏/制作等制作信息行；开头的歌手/专辑名也会跳过",
      default: true,
    },
    {
      key: "creditKeywords",
      type: "text",
      label: "制作人员关键词",
      description: "用逗号、分号或换行分隔。行首或行内出现“关键词:”时视为制作信息；默认已含常见中英文角色",
      default: DEFAULT_CREDIT_KEYWORDS,
      placeholder: "作词，作曲，编曲，Producer,...",
    },
    {
      key: "filterTrackNames",
      type: "switch",
      label: "过滤歌手/专辑署名",
      description: "短行包含歌手/专辑名时按制作信息处理，包括歌词段之后的署名行",
      default: true,
    },
    {
      key: "heartbeatSeconds",
      type: "number",
      label: "心跳间隔（秒）",
      description: "定时重发当前状态，让桥知道播放器仍在运行；建议 3–10 秒",
      default: 5,
      min: 2,
      max: 60,
    },
  ],
});

const DEFAULT_HOST = "127.0.0.1";
const DEFAULT_PORT = 33550;
const REQUEST_TIMEOUT_MS = 2000;

const sessionId =
  Math.random().toString(36).slice(2, 10) + Date.now().toString(36);


let lines = [];
let currentIndex = -1;
let currentTrack = null;
let playing = false;
let positionMs = 0;
let seq = 0;
let reportedFailure = false;

/** 读取设置，空字符串按默认值处理 */
const setting = (key, fallback) => {
  const value = splayer.getSetting(key);
  if (value === undefined || value === null || value === "") return fallback;
  return value;
};

const asBool = (key, fallback) => {
  const value = splayer.getSetting(key);
  return value === undefined || value === null ? fallback : Boolean(value);
};

const lineText = (line) => {
  if (!line || !Array.isArray(line.words)) return "";
  return line.words.map((word) => word.word || "").join("").trim();
};

const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** 用户关键词转成去重列表；空字符串表示不做关键词过滤 */
const creditKeywordList = () => {
  const raw = splayer.getSetting("creditKeywords");
  const text = raw === undefined || raw === null ? DEFAULT_CREDIT_KEYWORDS : String(raw);
  const seen = new Set();
  const keywords = [];
  for (const part of text.split(/[,，;；|\n]+/)) {
    const keyword = part.trim();
    if (!keyword) continue;
    const key = keyword.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    keywords.push(keyword);
  }
  return keywords;
};

let creditMatchers = null;
let creditMatcherKey = null;

const ensureCreditMatchers = () => {
  const keywords = creditKeywordList();
  // 关键词可能被设置页直接改写；用内容签名判断缓存是否还有效。
  const key = keywords.join("\u0001");
  if (creditMatchers && creditMatcherKey === key) return creditMatchers;
  creditMatcherKey = key;
  if (keywords.length === 0) {
    creditMatchers = { label: null, any: null, space: null };
    return creditMatchers;
  }
  const escaped = keywords.map(escapeRegExp);
  // 单字符/过短的关键词只认「关键词:」形式，避免误伤正常歌词。
  // 无冒号的「关键词 + 空格 + 内容」只对中文角色启用；英文歌词里
  // 「Music in the air」「Guitar solo」这类句子很常见，不能按空格误伤。
  const spaceKeywords = keywords
    .filter(
      (keyword) =>
        keyword.length > 1 &&
        /[^\x00-\x7F]/.test(keyword) &&
        !["OP", "SP", "Art"].includes(keyword),
    )
    .map(escapeRegExp);
  // 英文角色常带修饰词，如「Music Copyist：」「Produced by：」
  // 「Mixing Engineer：」；允许关键词和冒号之间夹少量英文词。
  const asciiKeywords = keywords
    .filter((keyword) => /^[\x00-\x7F]+$/.test(keyword) && keyword.length >= 3)
    .map(escapeRegExp);
  creditMatchers = {
    label: new RegExp(`^\\s*(?:${escaped.join("|")})\\s*[:：]`, "i"),
    any: new RegExp(`(?:${escaped.join("|")})\\s*[:：]`, "i"),
    space: spaceKeywords.length
      ? new RegExp(`^\\s*(?:${spaceKeywords.join("|")})\\s+\\S`, "i")
      : null,
    role: asciiKeywords.length
      ? new RegExp(`\\b(?:${asciiKeywords.join("|")})\\b[^:：]{0,30}[:：]`, "i")
      : null,
  };
  return creditMatchers;
};

let creditFlags = [];
let trackArtistNames = [];
let trackAlbumName = "";

const updateTrackMeta = () => {
  trackArtistNames = (currentTrack && Array.isArray(currentTrack.artists) ? currentTrack.artists : [])
    .map((artist) => (typeof artist === "string" ? artist : artist && artist.name))
    .filter(Boolean)
    .map((name) => name.trim().toLowerCase())
    .filter((name) => name.length >= 2);
  trackAlbumName =
    currentTrack && currentTrack.album && typeof currentTrack.album.name === "string"
      ? currentTrack.album.name.trim().toLowerCase()
      : "";
};

// 短歌手名（如 Nea）不能做子串匹配，否则 "beneath"/"near" 会命中 "nea"。
const containsWholeName = (lower, name) => {
  if (!name) return false;
  if (lower === name) return true;
  if (name.length < 2) return false;
  const pattern = new RegExp(
    `(^|[^a-z0-9])${escapeRegExp(name)}([^a-z0-9]|$)`,
    "i",
  );
  return pattern.test(lower);
};

const isCreditText = (line) => {
  const text = lineText(line);
  if (!text) return true;
  const matchers = ensureCreditMatchers();
  if (
    (matchers.label && matchers.label.test(text)) ||
    (matchers.any && matchers.any.test(text)) ||
    (matchers.space && matchers.space.test(text)) ||
    (matchers.role && matchers.role.test(text))
  ) {
    return true;
  }
  // 有翻译的行是正常歌词，不按歌手/专辑名过滤；否则与专辑同名的歌词会被误杀。
  const translated =
    typeof line.translatedLyric === "string" && line.translatedLyric.trim() !== "";
  if (!translated && asBool("filterTrackNames", true) && text.length <= 80) {
    const lower = text.toLowerCase();
    if (trackArtistNames.some((name) => containsWholeName(lower, name))) {
      return true;
    }
    if (trackAlbumName && containsWholeName(lower, trackAlbumName)) {
      return true;
    }
  }
  return false;
};

const rebuildCreditFlags = () => {
  const enabled = asBool("filterCredits", true);
  const flags = lines.map((line) => {
    const text = lineText(line);
    if (!text) return true;
    return enabled ? isCreditText(line) : false;
  });

  // 制作署名有时拆成两行，第二行没有冒号（如 "prime sound studio form"）。
  // 紧跟带冒号的制作行、且本身没有冒号的短行，也按制作信息处理。
  if (enabled) {
    for (let i = 1; i < flags.length; i += 1) {
      if (flags[i]) continue;
      const text = lineText(lines[i]);
      if (!text || /[:：]/.test(text) || text.length > 80) continue;
      const previous = lineText(lines[i - 1]);
      if (
        flags[i - 1] &&
        /[:：]/.test(previous) &&
        i + 1 < flags.length &&
        flags[i + 1]
      ) {
        flags[i] = true;
      }
    }
  }
  creditFlags = flags;
};

const textAt = (index) => {
  if (index < 0 || index >= lines.length || creditFlags[index]) return "";
  return lineText(lines[index]);
};

const isUsable = (index) => textAt(index) !== "";

const findPrevMainLineIndex = (from) => {
  for (let i = from - 1; i >= 0; i -= 1) {
    if (!lines[i].isBG && isUsable(i)) return i;
  }
  return -1;
};

const findOverlappingMainLineIndex = (from) => {
  const cur = lines[from];
  for (let i = from - 1; i >= 0; i -= 1) {
    const line = lines[i];
    if (line.isBG || line.isDuet || !isUsable(i)) continue;
    if (line.endTime > cur.startTime) return i;
  }
  return -1;
};

const findNextLineIndex = (from, skipBG) => {
  for (let i = from + 1; i < lines.length; i += 1) {
    if (skipBG && lines[i].isBG) continue;
    if (isUsable(i)) return i;
  }
  return -1;
};

const EMPTY_PICK = {
  current: "",
  currentTranslation: "",
  bgMain: "",
  bgMainTranslation: "",
  duetMain: "",
  duetMainTranslation: "",
  background: "",
  duet: "",
  next: "",
  isBG: false,
  isDuet: false,
  duetOverlap: false,
};

const translationOf = (line) =>
  line && typeof line.translatedLyric === "string" ? line.translatedLyric.trim() : "";

const pickLine = (index) => {
  const cur = lines[index];
  if (!cur) return { ...EMPTY_PICK };

  // 当前行是制作信息时，主行回退到上一条正常歌词，且不发送副行。
  if (!isUsable(index)) {
    const mainIndex = findPrevMainLineIndex(index);
    if (mainIndex < 0) return { ...EMPTY_PICK };
    return {
      ...EMPTY_PICK,
      current: textAt(mainIndex),
      currentTranslation: translationOf(lines[mainIndex]),
    };
  }

  const result = {
    ...EMPTY_PICK,
    current: textAt(index),
    currentTranslation: translationOf(cur),
    isBG: cur.isBG === true,
    isDuet: cur.isDuet === true,
  };

  if (result.isBG) {
    const mainIndex = findPrevMainLineIndex(index);
    if (mainIndex >= 0) {
      result.bgMain = textAt(mainIndex);
      result.bgMainTranslation = translationOf(lines[mainIndex]);
    }
    result.background = result.current;
  } else if (
    index + 1 < lines.length &&
    lines[index + 1].isBG &&
    isUsable(index + 1)
  ) {
    result.background = textAt(index + 1);
  }

  if (result.isDuet) {
    const mainIndex = findOverlappingMainLineIndex(index);
    if (mainIndex >= 0) {
      result.duetMain = textAt(mainIndex);
      result.duetMainTranslation = translationOf(lines[mainIndex]);
      result.duet = result.current;
      result.duetOverlap = true;
    }
  }

  const nextIndex = findNextLineIndex(index, true);
  if (nextIndex >= 0) result.next = textAt(nextIndex);

  return result;
};

const artistText = (track) => {
  if (!track || !Array.isArray(track.artists)) return "";
  return track.artists
    .map((artist) => (typeof artist === "string" ? artist : artist && artist.name))
    .filter(Boolean)
    .join(" / ");
};

const endpoint = () => {
  const configured = String(setting("host", DEFAULT_HOST)).trim() || DEFAULT_HOST;
  const host =
    configured.includes(":") && !configured.startsWith("[")
      ? `[${configured}]`
      : configured;
  let port = Number(setting("port", DEFAULT_PORT));
  if (!Number.isInteger(port) || port < 1 || port > 65535) port = DEFAULT_PORT;
  return `http://${host}:${port}/lyric`;
};

const buildPayload = () => {
  const picked = currentIndex >= 0 ? pickLine(currentIndex) : { ...EMPTY_PICK };
  return {
    session: sessionId,
    seq: (seq += 1),
    playing,
    title: currentTrack && typeof currentTrack.title === "string" ? currentTrack.title : "",
    artist: artistText(currentTrack),
    album:
      currentTrack && currentTrack.album && typeof currentTrack.album.name === "string"
        ? currentTrack.album.name
        : "",
    lyric: picked.current,
    extra: "",
    extraKind: "",
    protocol: 2,
    current: picked.current,
    currentTranslation: picked.currentTranslation,
    bgMain: picked.bgMain,
    bgMainTranslation: picked.bgMainTranslation,
    duetMain: picked.duetMain,
    duetMainTranslation: picked.duetMainTranslation,
    background: picked.background,
    duet: picked.duet,
    next: picked.next,
    isBG: picked.isBG,
    isDuet: picked.isDuet,
    duetOverlap: picked.duetOverlap,
    line: currentIndex,
    lineCount: lines.length,
    positionMs: Math.max(0, Math.round(positionMs || 0)),
    durationMs: Math.max(
      0,
      Math.round(
        currentTrack && Number.isFinite(currentTrack.duration) ? currentTrack.duration : 0,
      ),
    ),
    updatedAt: Date.now(),
  };
};

const send = async () => {
  const payload = buildPayload();
  try {
    await splayer.request(endpoint(), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      timeout: REQUEST_TIMEOUT_MS,
    });
    reportedFailure = false;
  } catch (error) {
    // 桥没开时每个心跳都会失败；只提醒第一次，恢复后重置。
    if (!reportedFailure) {
      reportedFailure = true;
      splayer.log.warn("Noctalia lyrics bridge unreachable:", error && error.message);
    }
  }
};

splayer.player.on("trackChange", ({ track }) => {
  currentTrack = track || null;
  lines = [];
  creditFlags = [];
  currentIndex = -1;
  positionMs = 0;
  updateTrackMeta();
  if (!track) playing = false;
  send();
});

splayer.player.on("lyricChange", ({ lines: nextLines }) => {
  lines = Array.isArray(nextLines) ? nextLines : [];
  rebuildCreditFlags();
  send();
});

splayer.player.on("lineChange", ({ index, position }) => {
  currentIndex = Number.isInteger(index) && index >= 0 ? index : -1;
  if (Number.isFinite(position)) positionMs = position;
  send();
});

splayer.player.on("playStateChange", ({ state, position }) => {
  playing = state === "playing";
  if (Number.isFinite(position)) positionMs = position;
  send();
});

splayer.onSettingChange("filterCredits", () => {
  creditMatchers = null;
  rebuildCreditFlags();
  send();
});

splayer.onSettingChange("creditKeywords", () => {
  creditMatchers = null;
  rebuildCreditFlags();
  send();
});

splayer.onSettingChange("filterTrackNames", () => {
  rebuildCreditFlags();
  send();
});

const heartbeatMs = Math.min(
  60000,
  Math.max(2000, Number(setting("heartbeatSeconds", 5)) * 1000),
);
setInterval(send, heartbeatMs);
