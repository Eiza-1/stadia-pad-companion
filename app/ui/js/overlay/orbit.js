// Keyboard B · Orbit rail: one row of characters glides under a big focus key. Left stick / D-pad moves,
// A types, B deletes, X adds a space, D-pad up / down chooses a suggestion and Y takes it, LB / RB switch letters ·
// numbers · emoji, L3 caps, Menu sends. Pure logic; orbit-view.js draws it and feeds pad edges.

import { RANKED } from "./words.js";

export const PAGES = {
  abc: [..."abcdefghijklmnopqrstuvwxyz"],
  "123": [..."0123456789.,?!'-:()"],
  emoji: [..."😂🔥👍💀🎮😭😎🤔😡😱🙏👏💪🤯🥶😴🤣😅😉😍🥳😤👀🫡✅❌⚡💥🏆🎯🧠💣⭐💯👋🙌🤝👌👊🎉"],
};
const ORDER = ["abc", "123", "emoji"];

/** Up to `n` words starting with `prefix` (case-insensitive), most useful first. */
export function suggest(prefix, n = 4) {
  const p = String(prefix).toLowerCase();
  if (!p) return [];
  return RANKED.filter((w) => w.startsWith(p) && w !== p).slice(0, n);
}

/** The word being typed at the end of `text` ("" after a space). */
const lastWord = (text) => (/(\S+)$/.exec(text) || [, ""])[1];

export function createOrbit() {
  let page = "abc";
  const pos = { abc: PAGES.abc.indexOf("t"), "123": 0, emoji: 0 };
  let caps = false;
  let text = "";
  let sel = 0; // chosen suggestion; back to the first whenever the text changes

  const chars = () => PAGES[page];
  const focus = () => {
    const c = chars()[pos[page]];
    return page === "abc" && caps ? c.toUpperCase() : c;
  };

  /** Puts suggestion `i` in place of the word being typed. */
  const pick = (i) => {
    const w = suggest(lastWord(text))[i];
    if (!w) return;
    text = text.slice(0, text.length - lastWord(text).length) + w + " ";
    sel = 0;
  };

  function key(edge) {
    const len = chars().length;
    switch (edge) {
      case "left":
        pos[page] = (pos[page] - 1 + len) % len;
        break;
      case "right":
        pos[page] = (pos[page] + 1) % len;
        break;
      case "up":
      case "down": {
        const n = suggest(lastWord(text)).length;
        sel = Math.max(0, Math.min(n - 1, sel + (edge === "down" ? 1 : -1)));
        break;
      }
      case "A":
        text += focus();
        break;
      case "B":
        text = [...text].slice(0, -1).join(""); // one character, even an emoji
        break;
      case "X":
        text += " ";
        break;
      case "Y":
        pick(sel);
        break;
      case "LB":
      case "RB": {
        const i = ORDER.indexOf(page) + (edge === "RB" ? 1 : -1);
        page = ORDER[(i + ORDER.length) % ORDER.length];
        break;
      }
      case "L3":
        caps = !caps;
        break;
      case "MENU":
        return { send: text };
    }
    return null;
  }

  return {
    get text() {
      return text;
    },
    set text(t) {
      text = String(t);
      sel = 0;
    },
    get state() {
      return { page, index: pos[page], focus: focus(), caps, suggestions: suggest(lastWord(text)), selected: sel };
    },
    /** Takes suggestion `i` (a click). */
    pick,
    /** One pad edge; returns `{ send }` for Menu. Any change to the text resets the chosen suggestion. */
    input(edge) {
      const before = text;
      const r = key(edge);
      if (text !== before) sel = 0;
      return r;
    },
  };
}
