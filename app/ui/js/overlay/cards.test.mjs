import { test } from "node:test";
import assert from "node:assert/strict";
import { isLong, createStack } from "./cards.js";

test("stack keeps the newest three, older ones marked old", () => {
  const s = createStack({ max: 3 });
  for (const id of [1, 2, 3, 4]) s.add(id, `q${id}`);
  assert.deepEqual(s.cards.map((c) => c.id), [2, 3, 4]);
  assert.deepEqual(s.cards.map((c) => c.old), [true, true, false]);
});

test("streamed text accumulates and the answered card stays until it is closed", () => {
  const s = createStack();
  s.add(1, "combo?");
  s.append(1, "Back, ");
  s.append(1, "Forward + 1");
  assert.equal(s.cards[0].text, "Back, Forward + 1");
  assert.equal(s.cards[0].done, false);
  s.done(1);
  assert.equal(s.cards[0].done, true);
  assert.equal("expire" in s, false, "nothing closes a card on a timer");
  s.close(1);
  assert.equal(s.cards.length, 0);
});

test("errors stay too, and close() removes a card", () => {
  const s = createStack();
  s.add(1, "q");
  s.error(1, "No internet connection");
  assert.equal(s.cards[0].error, "No internet connection");
  assert.equal(s.cards[0].done, true);
  s.add(2, "q2");
  s.close(1);
  assert.deepEqual(s.cards.map((c) => c.id), [2]);
  assert.equal(s.cards[0].old, false);
  s.closeAll();
  assert.equal(s.cards.length, 0);
});

test("updates for unknown ids are ignored", () => {
  const s = createStack();
  s.append(9, "x");
  s.done(9);
  s.toggle(9);
  assert.equal(s.cards.length, 0);
});

test("long answers collapse behind Show more; toggle opens and closes them", () => {
  assert.equal(isLong("Back, Forward + 1"), false);
  assert.equal(isLong("word ".repeat(60)), true, "over about four lines");
  assert.equal(isLong("1. a\n2. b\n3. c\n4. d\n5. e"), true, "more than four lines");
  const s = createStack();
  s.add(1, "q");
  assert.equal(s.cards[0].open, false);
  s.toggle(1);
  assert.equal(s.cards[0].open, true);
  s.toggle(1);
  assert.equal(s.cards[0].open, false);
});

test("read-aloud and voice errors are remembered on the card", () => {
  const s = createStack();
  s.add(1, "q");
  s.done(1);
  s.speaking(1);
  assert.equal(s.cards[0].voice, true);
  s.voiceError(1, "Gemini is busy right now. Try again in a moment");
  assert.equal(s.cards[0].voiceError, "Gemini is busy right now. Try again in a moment");
});
