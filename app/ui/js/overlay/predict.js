// Smarter keyboard suggestions for the Orbit rail: completes the word being typed, predicts the next
// word after a space, learns the words and word pairs the player actually sends (kept on this PC),
// forgives one typo, keeps a typed capital, and offers an emoji for words that have one. Pure; the
// store is injected (localStorage in the app, memory in tests).

import { RANKED } from "./words.js";

const N = 4;
const KEY = "pc.kb.learn.v1";

/** Common gaming-question phrases: where next-word guesses come from before anything is learned. */
const PHRASES = `
how do i|how do you|how to beat|how to get|how to unlock|how many|how much|how long|how does
what is the|what is a|what does|what do i|what level|what should i
where is the|where do i|where can i find|where to find|where to go
is there a|is it worth|can i|can you|should i|which is the
best build for|best weapon for|best way to|the best|the boss|the next|the final boss|the last
beat the boss|get past the|kill the|find the|open the|unlock the|upgrade the|level up
side quest|main quest|boss fight|skill tree|weak point|fast travel|save game|new game plus
i need|i am stuck|i can't find|i keep dying|i want to|tell me|give me|show me
in elden ring|in this game|on hard mode|for beginners|for a beginner|right now
thank you|good game|well played`;

/** Words that get an emoji suggestion (emoji from the bundled Noto 3D set). */
const EMOJI = {
  fire: "🔥", lit: "🔥", hot: "🔥", lol: "😂", lmao: "🤣", haha: "😂", funny: "😂", gg: "🤝", win: "🏆",
  won: "🏆", winner: "🏆", victory: "🏆", trophy: "🏆", dead: "💀", died: "💀", dying: "💀", rip: "💀", skull: "💀",
  cool: "😎", nice: "👌", ok: "👍", okay: "👍", yes: "👍", good: "👍", great: "👏", love: "😍", sad: "😭", cry: "😭",
  crying: "😭", angry: "😡", mad: "😡", rage: "😡", think: "🤔", hmm: "🤔", wow: "🤯", insane: "🤯", crazy: "🤯",
  cold: "🥶", sleep: "😴", tired: "😴", party: "🥳", thanks: "🙏", thank: "🙏", please: "🙏", strong: "💪",
  power: "⚡", fast: "⚡", boom: "💥", bomb: "💣", target: "🎯", aim: "🎯", brain: "🧠", smart: "🧠", star: "⭐",
  hundred: "💯", perfect: "💯", hi: "👋", hello: "👋", bye: "👋", game: "🎮", gaming: "🎮", controller: "🎮",
  scared: "😱", scary: "😱", wink: "😉", look: "👀", see: "👀", watch: "👀", salute: "🫡", done: "✅", no: "❌", wrong: "❌",
};

export const isEmoji = (s) => /\p{Extended_Pictographic}/u.test(s);

const words = (t) => String(t).toLowerCase().match(/[\p{L}\p{N}']+|\p{Extended_Pictographic}/gu) || [];

function distance(a, b) {
  const row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    let prev = row[0];
    row[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const cur = row[j];
      row[j] = Math.min(row[j] + 1, row[j - 1] + 1, prev + (a[i - 1] === b[j - 1] ? 0 : 1));
      prev = cur;
    }
  }
  return row[b.length];
}

const localStore = {
  get: () => { try { return localStorage.getItem(KEY); } catch { return null; } },
  set: (v) => { try { localStorage.setItem(KEY, v); } catch { /* full or blocked: just don't learn */ } },
};

export function createPredictor({ store = localStore } = {}) {
  const rank = new Map(RANKED.map((w, i) => [w, i]));
  const builtIn = new Map(); // "prev next" -> count
  for (const p of PHRASES.split(/[|\n]/)) {
    const w = words(p);
    for (let i = 0; i + 1 < w.length; i++) builtIn.set(`${w[i]} ${w[i + 1]}`, (builtIn.get(`${w[i]} ${w[i + 1]}`) || 0) + 1);
    for (const x of w) if (!rank.has(x)) rank.set(x, RANKED.length + 50);
  }
  let learned = { uni: {}, bi: {}, emoji: {} };
  try { learned = { ...learned, ...JSON.parse(store.get() || "{}") }; } catch { /* corrupt: start fresh */ }

  const vocab = () => new Set([...rank.keys(), ...Object.keys(learned.uni).filter((w) => !isEmoji(w))]);
  const score = (w, prev) =>
    1 / (1 + (rank.has(w) ? rank.get(w) : 5000) / 40) +
    3 * (builtIn.get(`${prev} ${w}`) || 0) +
    1.5 * Math.log1p(learned.uni[w] || 0) +
    4 * Math.log1p(learned.bi[`${prev} ${w}`] || 0);

  function nextWords(prev) {
    const out = new Map();
    for (const [k, c] of builtIn) if (k.startsWith(`${prev} `)) out.set(k.slice(prev.length + 1), (out.get(k.slice(prev.length + 1)) || 0) + 3 * c);
    for (const [k, c] of Object.entries(learned.bi)) if (k.startsWith(`${prev} `) && !isEmoji(k.slice(prev.length + 1))) out.set(k.slice(prev.length + 1), (out.get(k.slice(prev.length + 1)) || 0) + 4 * c);
    return [...out].sort((a, b) => b[1] - a[1]).map(([w]) => w);
  }

  function emojiFor(text, typing) {
    const w = words(text).filter((x) => !isEmoji(x));
    const key = typing ? w[w.length - 1] : w[w.length - 1];
    const e = key && EMOJI[key];
    return e && !String(text).includes(e) ? e : null;
  }

  return {
    /** Up to four suggestions for the text so far (words, maybe one emoji last). */
    suggest(text) {
      const t = String(text);
      const typing = /[\p{L}\p{N}']$/u.test(t);
      const all = words(t);
      const prefixRaw = typing ? (t.match(/[\p{L}\p{N}']+$/u) || [""])[0] : "";
      const prefix = prefixRaw.toLowerCase();
      const prev = (typing ? all[all.length - 2] : all[all.length - 1]) || "";
      let out;
      if (!prefix) {
        out = nextWords(prev);
        if (out.length < N) for (const w of ["how", "what", "where", "is", "can", "the", "best"]) if (!out.includes(w)) out.push(w);
      } else {
        const v = [...vocab()];
        out = v.filter((w) => w.startsWith(prefix) && w !== prefix).sort((a, b) => score(b, prev) - score(a, prev));
        if (out.length < 2 && prefix.length >= 4) {
          const near = v.filter((w) => !out.includes(w) && w !== prefix &&
            (distance(prefix, w) <= 1 || distance(prefix, w.slice(0, prefix.length)) <= 1)).sort((a, b) => score(b, prev) - score(a, prev));
          out = out.concat(near);
        }
        if (/^\p{Lu}/u.test(prefixRaw)) out = out.map((w) => w[0].toUpperCase() + w.slice(1));
      }
      const emoji = emojiFor(t, typing);
      out = out.slice(0, emoji ? N - 1 : N);
      return emoji ? [...out, emoji] : out;
    },
    /** A sent question: remember its words, word pairs and emoji. */
    learn(text) {
      const w = words(text);
      for (let i = 0; i < w.length; i++) {
        if (isEmoji(w[i])) learned.emoji[w[i]] = (learned.emoji[w[i]] || 0) + 1;
        else learned.uni[w[i]] = (learned.uni[w[i]] || 0) + 1;
        if (i > 0) learned.bi[`${w[i - 1]} ${w[i]}`] = (learned.bi[`${w[i - 1]} ${w[i]}`] || 0) + 1;
      }
      store.set(JSON.stringify(learned));
    },
    /** The emoji page with the player's most used emoji first. */
    emojiOrder(page) {
      return page.map((e, i) => [e, i]).sort((a, b) => (learned.emoji[b[0]] || 0) - (learned.emoji[a[0]] || 0) || a[1] - b[1]).map(([e]) => e);
    },
  };
}
