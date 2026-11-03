// Answer cards above the pill: the newest in front, older ones cascade back (up to `max`). A card stays
// until the player closes it (× on the card, or B / Esc with the pill open); long answers collapse
// behind "Show more". Pure model; main.js draws it.

/** Long enough to collapse: more than about four lines of text in the card. */
export function isLong(text) {
  const t = String(text);
  return t.length > 220 || t.split("\n").length > 4;
}

export function createStack({ max = 3 } = {}) {
  const cards = [];
  const find = (id) => cards.find((c) => c.id === id);
  const mark = () => cards.forEach((c, i) => (c.old = i < cards.length - 1));

  return {
    cards,
    add(id, question) {
      cards.push({ id, question, text: "", error: null, done: false, open: false, old: false, voice: false, voiceError: null });
      while (cards.length > max) cards.shift();
      mark();
    },
    append(id, delta) {
      const c = find(id);
      if (c) c.text += delta;
    },
    done(id) {
      const c = find(id);
      if (c) c.done = true;
    },
    error(id, msg) {
      const c = find(id);
      if (c) {
        c.error = msg;
        c.done = true;
      }
    },
    /** Show more / Show less. */
    toggle(id) {
      const c = find(id);
      if (c) c.open = !c.open;
    },
    /** Gemini's voice started reading card `id`. */
    speaking(id) {
      const c = find(id);
      if (c) c.voice = true;
    },
    /** The voice failed: the card shows its text after all, with this note. */
    voiceError(id, msg) {
      const c = find(id);
      if (c) c.voiceError = msg;
    },
    close(id) {
      const i = cards.findIndex((c) => c.id === id);
      if (i >= 0) cards.splice(i, 1);
      mark();
    },
    closeAll() {
      cards.length = 0;
    },
  };
}
