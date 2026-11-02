// Keyboard B · Orbit rail: one row of characters glides under a big focus key. Left stick / D-pad moves,
// A types, B deletes, X adds a space, Y takes the highlighted suggestion, LB / RB switch letters ·
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

  const chars = () => PAGES[page];
  const focus = () => {
    const c = chars()[pos[page]];
    return page === "abc" && caps ? c.toUpperCase() : c;
  };

  return {
    get text() {
      return text;
    },
    set text(t) {
      text = String(t);
    },
    get state() {
      return { page, index: pos[page], focus: focus(), caps, suggestions: suggest(lastWord(text)) };
    },
    /** One pad edge; returns `{ send }` for Menu. */
    input(edge) {
      const len = chars().length;
      switch (edge) {
        case "left":
          pos[page] = (pos[page] - 1 + len) % len;
          break;
        case "right":
          pos[page] = (pos[page] + 1) % len;
          break;
        case "A":
          text += focus();
          break;
        case "B":
          text = [...text].slice(0, -1).join(""); // one character, even an emoji
          break;
        case "X":
          text += " ";
          break;
        case "Y": {
          const w = suggest(lastWord(text))[0];
          if (w) text = text.slice(0, text.length - lastWord(text).length) + w + " ";
          break;
        }
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
    },
  };
}
