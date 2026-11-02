// Answer cards above the pill: the newest in front, older ones cascade back (up to `max`), each fades on
// its own once answered (longer answers stay longer) or is closed with ×. Pure model; main.js draws it.

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** How long an answered card stays: 4 s plus 60 ms a word, between 6 and 20 s. */
export function fadeMs(text) {
  const words = String(text).trim().split(/\s+/).filter(Boolean).length;
  return clamp(4000 + 60 * words, 6000, 20000);
}

const ERROR_MS = 6000;

export function createStack({ max = 3, now = () => Date.now() } = {}) {
  const cards = [];
  const find = (id) => cards.find((c) => c.id === id);
  const mark = () => cards.forEach((c, i) => (c.old = i < cards.length - 1));

  return {
    cards,
    add(id, question) {
      cards.push({ id, question, text: "", error: null, dueAt: null, old: false });
      while (cards.length > max) cards.shift();
      mark();
    },
    append(id, delta) {
      const c = find(id);
      if (c) c.text += delta;
    },
    done(id) {
      const c = find(id);
      if (c) c.dueAt = now() + fadeMs(c.text);
    },
    error(id, msg) {
      const c = find(id);
      if (c) {
        c.error = msg;
        c.dueAt = now() + ERROR_MS;
      }
    },
    close(id) {
      const i = cards.findIndex((c) => c.id === id);
      if (i >= 0) cards.splice(i, 1);
      mark();
    },
    closeAll() {
      cards.length = 0;
    },
    /** Removes cards whose time is up; returns their ids. */
    expire() {
      const t = now();
      const gone = cards.filter((c) => c.dueAt != null && c.dueAt <= t).map((c) => c.id);
      for (const id of gone) this.close(id);
      return gone;
    },
  };
}
