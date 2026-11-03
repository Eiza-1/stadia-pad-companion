import { test } from "node:test";
import assert from "node:assert/strict";
import { fadeMs, createStack } from "./cards.js";

test("fadeMs grows with length within 6-20 s", () => {
  assert.equal(fadeMs("one two"), 6000);
  assert.equal(fadeMs(Array(400).fill("w").join(" ")), 20000);
  assert.equal(fadeMs(Array(50).fill("w").join(" ")), 7000);
});

test("stack keeps the newest three, older ones marked old", () => {
  const s = createStack({ max: 3, now: () => 0 });
  for (const id of [1, 2, 3, 4]) s.add(id, `q${id}`);
  assert.deepEqual(s.cards.map((c) => c.id), [2, 3, 4]);
  assert.deepEqual(s.cards.map((c) => c.old), [true, true, false]);
});

test("streamed text accumulates and fades after done", () => {
  let t = 0;
  const s = createStack({ now: () => t });
  s.add(1, "combo?");
  s.append(1, "Back, ");
  s.append(1, "Forward + 1");
  assert.equal(s.cards[0].text, "Back, Forward + 1");
  assert.equal(s.cards[0].dueAt, null, "no fade while streaming");
  s.done(1);
  assert.equal(s.cards[0].dueAt, 6000);
  t = 5999;
  assert.deepEqual(s.expire(), []);
  t = 6000;
  assert.deepEqual(s.expire(), [1]);
  assert.equal(s.cards.length, 0);
});

test("errors show for 6 s and close() removes a card", () => {
  const s = createStack({ now: () => 100 });
  s.add(1, "q");
  s.error(1, "No internet connection");
  assert.equal(s.cards[0].error, "No internet connection");
  assert.equal(s.cards[0].dueAt, 6100);
  s.add(2, "q2");
  s.close(1);
  assert.deepEqual(s.cards.map((c) => c.id), [2]);
  assert.equal(s.cards[0].old, false);
  s.closeAll();
  assert.equal(s.cards.length, 0);
});

test("updates for unknown ids are ignored", () => {
  const s = createStack({ now: () => 0 });
  s.append(9, "x");
  s.done(9);
  assert.equal(s.cards.length, 0);
});

test("a card being read aloud stays until the voice ends", () => {
  let t = 0;
  const s = createStack({ now: () => t });
  s.add(1, "q");
  s.append(1, "short");
  s.done(1);
  s.speaking(1, 30000);
  t = 29000;
  assert.deepEqual(s.expire(), [], "still speaking");
  t = 32000;
  assert.deepEqual(s.expire(), [1]);
});

test("speaking never shortens a card, and a failed voice is remembered", () => {
  let t = 0;
  const s = createStack({ now: () => t });
  s.add(1, "q");
  s.append(1, "word ".repeat(100));
  s.done(1);
  const due = s.cards[0].dueAt;
  s.speaking(1, 10);
  assert.equal(s.cards[0].dueAt, due);
  s.voiceError(1, "Gemini is busy right now. Try again in a moment");
  assert.equal(s.cards[0].voiceError, "Gemini is busy right now. Try again in a moment");
});
