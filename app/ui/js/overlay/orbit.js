// Keyboard B · Orbit rail: one row of characters glides under a big focus key. Left stick / D-pad moves,
// A types, B deletes, X adds a space, D-pad up / down chooses a suggestion and Y takes it, LB / RB switch letters ·
// numbers · emoji, L3 caps, Menu sends. Pure logic; orbit-view.js draws it and feeds pad edges.

import { createPredictor, isEmoji } from "./predict.js";

export const PAGES = {
  abc: [..."abcdefghijklmnopqrstuvwxyz"],
  "123": [..."0123456789.,?!'-:()"],
  emoji: [..."😂🔥👍💀🎮😭😎🤔😡😱🙏👏💪🤯🥶😴🤣😅😉😍🥳😤👀🫡✅❌⚡💥🏆🎯🧠💣⭐💯👋🙌🤝👌👊🎉"],
};
const ORDER = ["abc", "123", "emoji"];

/** The word being typed at the end of `text` ("" after a space). */
const lastWord = (text) => (/(\S+)$/.exec(text) || [, ""])[1];

export function createOrbit({ predictor = createPredictor() } = {}) {
  let page = "abc";
  const pos = { abc: PAGES.abc.indexOf("t"), "123": 0, emoji: 0 };
  let caps = false;
  let text = "";
  let sel = 0; // chosen suggestion; back to the first whenever the text changes

  const emoji = predictor.emojiOrder(PAGES.emoji); // the player's most used emoji first
  const chars = () => (page === "emoji" ? emoji : PAGES[page]);
  const focus = () => {
    const c = chars()[pos[page]];
    return page === "abc" && caps ? c.toUpperCase() : c;
  };

  const suggestions = () => predictor.suggest(text);
  /** Suggestion `i`: a word replaces the one being typed (or follows a space); an emoji goes after it. */
  const pick = (i) => {
    const w = suggestions()[i];
    if (!w) return;
    if (isEmoji(w)) text = text + (text && !/\s$/.test(text) ? " " : "") + w + " ";
    else text = text.slice(0, text.length - lastWord(text).length) + w + " ";
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
        const n = suggestions().length;
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
      return { page, chars: chars(), index: pos[page], focus: focus(), caps, suggestions: suggestions(), selected: sel };
    },
    /** Takes suggestion `i` (a click). */
    pick,
    /** A question was sent: the keyboard learns its words. */
    learn: (t) => predictor.learn(t),
    /** The emoji page, most used first. */
    emojiPage: () => predictor.emojiOrder(PAGES.emoji),
    /** One pad edge; returns `{ send }` for Menu. Any change to the text resets the chosen suggestion. */
    input(edge) {
      const before = text;
      const r = key(edge);
      if (text !== before) sel = 0;
      return r;
    },
  };
}
