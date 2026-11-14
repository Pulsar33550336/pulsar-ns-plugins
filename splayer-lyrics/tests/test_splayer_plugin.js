#!/usr/bin/env node
/**
 * SPlayer-Next 插件的离线测试：用假的 `splayer` 对象加载脚本，触发播放事件，
 * 检查它 POST 给桥的 payload。只验证纯 JS 逻辑，不需要安装到 SPlayer-Next。
 *
 * 运行：node tests/test_splayer_plugin.js
 */

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const scriptPath = path.join(__dirname, "..", "splayer-plugin", "splayer-noctalia-lyrics.js");
const source = fs.readFileSync(scriptPath, "utf8");

const handlers = new Map();
const posts = [];
const logs = [];
const intervals = [];
let registration = null;
let settings = {
  host: "127.0.0.1",
  port: 33550,
  filterCredits: true,
  filterTrackNames: true,
  heartbeatSeconds: 5,
};

const splayer = {
  register(args) {
    registration = args;
  },
  getSetting(key) {
    return settings[key];
  },
  onSettingChange() {},
  request(url, options) {
    posts.push({ url, options, body: JSON.parse(options.body) });
    return Promise.resolve({ status: 200, headers: {}, body: "" });
  },
  player: {
    on(kind, handler) {
      handlers.set(kind, handler);
    },
  },
  log: {
    debug() {},
    info() {},
    warn(...args) {
      logs.push(args.join(" "));
    },
    error() {},
  },
};

const sandbox = {
  splayer,
  console,
  setInterval(fn, ms) {
    intervals.push({ fn, ms });
    return intervals.length;
  },
  clearInterval() {},
};
vm.createContext(sandbox);
vm.runInContext(source, sandbox, { filename: scriptPath });

const tick = () => new Promise((resolve) => setImmediate(resolve));
const settle = async () => {
  for (let i = 0; i < 5; i += 1) await tick();
};
const last = () => posts[posts.length - 1].body;

const track = {
  id: "1",
  title: "Test Song",
  artists: [{ name: "A" }, { name: "B" }],
  album: { name: "Album" },
  duration: 200000,
};

const lines = [
  { words: [{ word: "A" }], translatedLyric: "A译", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
  { words: [{ word: "bg" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: true, isDuet: false },
  { words: [{ word: "B" }], translatedLyric: "B译", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
  { words: [{ word: "D" }], translatedLyric: "", startTime: 2500, endTime: 3500, isBG: false, isDuet: true },
  { words: [{ word: "C" }], translatedLyric: "", startTime: 3000, endTime: 4000, isBG: false, isDuet: false },
];

(async () => {
  assert.ok(registration, "plugin should call splayer.register");
  assert.equal(
    Array.from(registration.events).join(","),
    "trackChange,lyricChange,lineChange,playStateChange",
  );
  assert.equal(registration.settings.length, 6);
  assert.equal(intervals.length, 1, "plugin should set one heartbeat timer");
  assert.equal(intervals[0].ms, 5000);

  handlers.get("trackChange")({ track });
  await settle();
  assert.equal(last().title, "Test Song");
  assert.equal(last().artist, "A / B");
  assert.equal(last().album, "Album");
  assert.equal(last().playing, false);

  handlers.get("lyricChange")({ lines });
  handlers.get("playStateChange")({ state: "playing", position: 1500 });
  await settle();
  assert.equal(last().playing, true);
  assert.equal(last().positionMs, 1500);

  // 背景行：发送所属主行，以及当前背景行作为候选。
  handlers.get("lineChange")({ index: 1, position: 1500 });
  await settle();
  assert.equal(last().protocol, 2);
  assert.equal(last().current, "bg");
  assert.equal(last().isBG, true);
  assert.equal(last().bgMain, "A");
  assert.equal(last().bgMainTranslation, "A译");
  assert.equal(last().background, "bg");

  // 普通行：发送当前行、翻译和下一行，具体显示由 Noctalia 决定。
  handlers.get("lineChange")({ index: 2, position: 2100 });
  await settle();
  assert.equal(last().current, "B");
  assert.equal(last().currentTranslation, "B译");
  assert.equal(last().isBG, false);
  assert.equal(last().isDuet, false);
  assert.equal(last().next, "D");

  // 对唱行：发送重叠的主行与对唱候选。
  handlers.get("lineChange")({ index: 3, position: 2600 });
  await settle();
  assert.equal(last().current, "D");
  assert.equal(last().isDuet, true);
  assert.equal(last().duetOverlap, true);
  assert.equal(last().duetMain, "B");
  assert.equal(last().duetMainTranslation, "B译");
  assert.equal(last().duet, "D");

  // 第一行后面紧跟背景歌词：背景候选随行发送。
  handlers.get("lineChange")({ index: 0, position: 100 });
  await settle();
  assert.equal(last().current, "A");
  assert.equal(last().currentTranslation, "A译");
  assert.equal(last().background, "bg");
  assert.equal(last().next, "B");

  // 无匹配行：候选清空，但歌名/歌手保留。
  handlers.get("lineChange")({ index: -1, position: 100 });
  await settle();
  assert.equal(last().current, "");
  assert.equal(last().lyric, "");
  assert.equal(last().line, -1);
  assert.equal(last().title, "Test Song");

  // 心跳复用当前状态，并递增 seq。
  const before = last().seq;
  intervals[0].fn();
  await settle();
  assert.equal(last().seq, before + 1);

  // 端口设置生效；非法值回到默认端口。
  settings.port = 40000;
  handlers.get("playStateChange")({ state: "paused", position: 1600 });
  await settle();
  assert.match(posts[posts.length - 1].url, /^http:\/\/127\.0\.0\.1:40000\/lyric$/);
  assert.equal(last().playing, false);

  settings.port = "not-a-port";
  handlers.get("playStateChange")({ state: "playing", position: 1700 });
  await settle();
  assert.match(posts[posts.length - 1].url, /^http:\/\/127\.0\.0\.1:33550\/lyric$/);

  // 制作人员过滤：歌手名、常见中英文角色、带冒号的行都跳过。
  const creditLines = [
    { words: [{ word: "HOYO-MiX" }], translatedLyric: "", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "萨克斯 Sax：李世海" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
    { words: [{ word: "小号 Trumpet：胡丹峰" }], translatedLyric: "", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
    { words: [{ word: "作词 : 张三" }], translatedLyric: "", startTime: 3000, endTime: 4000, isBG: false, isDuet: false },
    { words: [{ word: "The Elation never dies" }], translatedLyric: "", startTime: 4000, endTime: 5000, isBG: false, isDuet: false },
    { words: [{ word: "Music in the air" }], translatedLyric: "", startTime: 5000, endTime: 6000, isBG: false, isDuet: false },
  ];
  const creditTrack = {
    ...track,
    artists: [{ name: "HOYO-MiX" }],
    album: { name: "Album" },
  };
  handlers.get("trackChange")({ track: creditTrack });
  handlers.get("lyricChange")({ lines: creditLines });
  handlers.get("playStateChange")({ state: "playing", position: 100 });
  await settle();
  for (const index of [0, 1, 2, 3]) {
    handlers.get("lineChange")({ index, position: index * 1000 + 100 });
    await settle();
    assert.equal(last().lyric, "", `credit line ${index} should be filtered`);
  }
  handlers.get("lineChange")({ index: 4, position: 4100 });
  await settle();
  assert.equal(last().lyric, "The Elation never dies");

  // 英文关键词只有「关键词:」才过滤，不能把「Music in the air」当制作信息。
  handlers.get("lineChange")({ index: 5, position: 5100 });
  await settle();
  assert.equal(last().lyric, "Music in the air");

  // 乐队/乐团署名也要过滤。
  const orchestraLines = [
    { words: [{ word: "乐队 Orchestra：国际首席爱乐乐团 International Master Philharmonic Orchestra" }], translatedLyric: "", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "Real line" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
    { words: [{ word: "Real next" }], translatedLyric: "", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: orchestraLines });
  handlers.get("lineChange")({ index: 0, position: 100 });
  await settle();
  assert.equal(last().lyric, "");
  handlers.get("lineChange")({ index: 1, position: 1100 });
  await settle();
  assert.equal(last().lyric, "Real line");

  // 民族管乐署名也要过滤。
  const ethnicLines = [
    { words: [{ word: "民族管乐 Ethnic Wind Instruments: Sandro Friedrich" }], translatedLyric: "", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "Real line" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
    { words: [{ word: "Real next" }], translatedLyric: "", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: ethnicLines });
  handlers.get("lineChange")({ index: 0, position: 100 });
  await settle();
  assert.equal(last().lyric, "");
  handlers.get("lineChange")({ index: 1, position: 1100 });
  await settle();
  assert.equal(last().lyric, "Real line");

  // 与专辑同名的正常歌词如果有翻译，不能被 filterTrackNames 误杀。
  const albumTrack = {
    ...creditTrack,
    album: { name: "We dance beneath the galaxy" },
  };
  handlers.get("trackChange")({ track: albumTrack });
  const albumLyricLines = [
    { words: [{ word: "We dance beneath the galaxy" }], translatedLyric: "我们在银河下跳舞", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "Real next" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: albumLyricLines });
  handlers.get("lineChange")({ index: 0, position: 100 });
  await settle();
  assert.equal(last().lyric, "We dance beneath the galaxy");
  assert.equal(last().currentTranslation, "我们在银河下跳舞");

  // 短歌手名 Nea 不能子串命中 beneath/near。
  const neaTrack = {
    ...creditTrack,
    artists: [{ name: "Nea" }],
    album: { name: "Album" },
  };
  handlers.get("trackChange")({ track: neaTrack });
  const neaLines = [
    { words: [{ word: "We dance beneath the galaxy" }], translatedLyric: "", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "Conquering doubts that linger near" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
    { words: [{ word: "Nea" }], translatedLyric: "", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
    { words: [{ word: "Real line" }], translatedLyric: "", startTime: 3000, endTime: 4000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: neaLines });
  for (const index of [0, 1]) {
    handlers.get("lineChange")({ index, position: index * 1000 + 100 });
    await settle();
    assert.equal(last().lyric, neaLines[index].words[0].word);
  }
  handlers.get("lineChange")({ index: 2, position: 2100 });
  await settle();
  assert.equal(last().lyric, "Conquering doubts that linger near");
  handlers.get("lineChange")({ index: 3, position: 3100 });
  await settle();
  assert.equal(last().lyric, "Real line");

  // 制作信息前面带了前缀（例如「W HOYO-MiX」）也要按歌手名过滤。
  handlers.get("trackChange")({ track: creditTrack });
  const wrappedLines = [
    { words: [{ word: "W HOYO-MiX" }], translatedLyric: "", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "Real line" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: wrappedLines });
  handlers.get("lineChange")({ index: 0, position: 100 });
  await settle();
  assert.equal(last().lyric, "");
  handlers.get("lineChange")({ index: 1, position: 1100 });
  await settle();
  assert.equal(last().lyric, "Real line");

  // 真实 MP3 标签里的整段制作信息：W/S 署名、制谱、出品、录音棚等。
  const fullCreditBlock = [
    { words: [{ word: "坠星的摇篮 Fallen Star's Cradle - HOYO-MiX" }], translatedLyric: "", startTime: 0, endTime: 3000, isBG: false, isDuet: false },
    { words: [{ word: "作曲 Composer：温菁 Jing." }], translatedLyric: "", startTime: 3000, endTime: 5000, isBG: false, isDuet: false },
    { words: [{ word: "W HOYO-MiX" }], translatedLyric: "", startTime: 5000, endTime: 6000, isBG: false, isDuet: false },
    { words: [{ word: "编曲 Arranger：温菁 Jing." }], translatedLyric: "", startTime: 6000, endTime: 8000, isBG: false, isDuet: false },
    { words: [{ word: "W HOYO-MiX" }], translatedLyric: "", startTime: 8000, endTime: 9000, isBG: false, isDuet: false },
    { words: [{ word: "制谱 Music Copyist：沈梦吟 Sherry." }], translatedLyric: "", startTime: 9000, endTime: 11000, isBG: false, isDuet: false },
    { words: [{ word: "S HOYO-MiX" }], translatedLyric: "", startTime: 11000, endTime: 12000, isBG: false, isDuet: false },
    { words: [{ word: "人声 Vocal：梦西 Maisy" }], translatedLyric: "", startTime: 12000, endTime: 14000, isBG: false, isDuet: false },
    { words: [{ word: "吉他 Guitar：范晔" }], translatedLyric: "", startTime: 14000, endTime: 16000, isBG: false, isDuet: false },
    { words: [{ word: "录音棚 Recording Studio：升赫录音棚 Soundhub Studio" }], translatedLyric: "", startTime: 16000, endTime: 20000, isBG: false, isDuet: false },
    { words: [{ word: "录音师 Recording Engineer：吴身宝 Bob Wu@Soundhub Studios" }], translatedLyric: "", startTime: 20000, endTime: 24000, isBG: false, isDuet: false },
    { words: [{ word: "混音师 Mixing Engineer：温菁 Jing." }], translatedLyric: "", startTime: 24000, endTime: 26000, isBG: false, isDuet: false },
    { words: [{ word: "W HOYO-MiX" }], translatedLyric: "", startTime: 26000, endTime: 27000, isBG: false, isDuet: false },
    { words: [{ word: "母带制作 Mastering Engineer：Steve Fallone@SterlingSound" }], translatedLyric: "", startTime: 27000, endTime: 30000, isBG: false, isDuet: false },
    { words: [{ word: "出品 Produced by：HOYO-MiX" }], translatedLyric: "", startTime: 30000, endTime: 32000, isBG: false, isDuet: false },
    { words: [{ word: "弦乐四重奏 String Quartet：刘星弦乐团" }], translatedLyric: "", startTime: 32000, endTime: 33000, isBG: false, isDuet: false },
    { words: [{ word: "Actual lyric" }], translatedLyric: "", startTime: 33000, endTime: 35000, isBG: false, isDuet: false },
    { words: [{ word: "Real next" }], translatedLyric: "", startTime: 35000, endTime: 37000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: fullCreditBlock });
  for (let index = 0; index < 16; index += 1) {
    handlers.get("lineChange")({ index, position: index * 1000 + 100 });
    await settle();
    assert.equal(last().lyric, "", `full credit block line ${index} should be filtered`);
  }
  handlers.get("lineChange")({ index: 16, position: 33100 });
  await settle();
  assert.equal(last().lyric, "Actual lyric");
  handlers.get("lineChange")({ index: 17, position: 35100 });
  await settle();
  assert.equal(last().lyric, "Real next");

  // 歌词段之后的短署名行（如单独的 HOYO-MiX）也要按歌手名过滤。
  const lateCreditLines = [
    { words: [{ word: "Real lyric" }], translatedLyric: "真实歌词", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "HOYO-MiX" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
    { words: [{ word: "Real next" }], translatedLyric: "", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: lateCreditLines });
  handlers.get("lineChange")({ index: 0, position: 100 });
  await settle();
  assert.equal(last().lyric, "Real lyric");
  handlers.get("lineChange")({ index: 1, position: 1100 });
  await settle();
  assert.equal(last().lyric, "Real lyric");
  assert.equal(last().currentTranslation, "真实歌词");
  assert.equal(last().background, "");
  assert.equal(last().next, "");
  handlers.get("lineChange")({ index: 2, position: 2100 });
  await settle();
  assert.equal(last().lyric, "Real next");

  // 制作署名拆成第二行时也过滤；当前行回退到上一条正常歌词。
  const continuationLines = [
    { words: [{ word: "Real lyric" }], translatedLyric: "真实歌词", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "人声录音师 Recording Engineer：Someone" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
    { words: [{ word: "prime sound studio form" }], translatedLyric: "", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
    { words: [{ word: "混音师 Mixing Engineer：Someone" }], translatedLyric: "", startTime: 3000, endTime: 4000, isBG: false, isDuet: false },
    { words: [{ word: "Real next" }], translatedLyric: "", startTime: 4000, endTime: 5000, isBG: false, isDuet: false },
    { words: [{ word: "Real next 2" }], translatedLyric: "", startTime: 5000, endTime: 6000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: continuationLines });
  for (const index of [1, 2, 3]) {
    handlers.get("lineChange")({ index, position: index * 1000 + 100 });
    await settle();
    assert.equal(last().lyric, "Real lyric");
    assert.equal(last().currentTranslation, "真实歌词");
    assert.equal(last().background, "");
    assert.equal(last().next, "");
  }
  handlers.get("lineChange")({ index: 4, position: 4100 });
  await settle();
  assert.equal(last().lyric, "Real next");
  handlers.get("lineChange")({ index: 5, position: 5100 });
  await settle();
  assert.equal(last().lyric, "Real next 2");

  // 拆行署名的下一行是最后一行时也按制作信息处理。
  const lastLineCredit = [
    { words: [{ word: "Real lyric" }], translatedLyric: "真实歌词", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "人声录音师 Recording Engineer：Someone" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
    { words: [{ word: "prime sound studio form" }], translatedLyric: "", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: lastLineCredit });
  handlers.get("lineChange")({ index: 2, position: 2100 });
  await settle();
  assert.equal(last().lyric, "Real lyric");
  assert.equal(last().currentTranslation, "真实歌词");

  // 关掉过滤后，制作人员行恢复显示。
  settings.filterCredits = false;
  handlers.get("lyricChange")({ lines: creditLines });
  handlers.get("lineChange")({ index: 0, position: 100 });
  await settle();
  assert.equal(last().lyric, "HOYO-MiX");

  // 关键词是用户可改的字面量；正则元字符要按字面匹配。
  settings.filterCredits = true;
  settings.creditKeywords = "CustomRole, A+B";
  const customLines = [
    { words: [{ word: "CustomRole: X" }], translatedLyric: "", startTime: 0, endTime: 1000, isBG: false, isDuet: false },
    { words: [{ word: "A+B: Y" }], translatedLyric: "", startTime: 1000, endTime: 2000, isBG: false, isDuet: false },
    { words: [{ word: "AAAB: Z" }], translatedLyric: "", startTime: 2000, endTime: 3000, isBG: false, isDuet: false },
    { words: [{ word: "Real next" }], translatedLyric: "", startTime: 3000, endTime: 4000, isBG: false, isDuet: false },
  ];
  handlers.get("lyricChange")({ lines: customLines });
  for (const index of [0, 1]) {
    handlers.get("lineChange")({ index, position: index * 1000 + 100 });
    await settle();
    assert.equal(last().lyric, "", `custom credit line ${index} should be filtered`);
  }
  handlers.get("lineChange")({ index: 2, position: 2100 });
  await settle();
  assert.equal(last().lyric, "AAAB: Z");

  // 关键词清空、关掉歌手/专辑名过滤后，开头的歌手名应显示出来。
  settings.creditKeywords = "";
  settings.filterTrackNames = false;
  handlers.get("lyricChange")({ lines: creditLines });
  handlers.get("lineChange")({ index: 0, position: 100 });
  await settle();
  assert.equal(last().lyric, "HOYO-MiX");

  console.log(`ok: ${posts.length} payloads checked`);
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
