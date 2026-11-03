import { test } from "node:test";
import assert from "node:assert/strict";
import { createPredictor, isEmoji } from "./predict.js";

const memStore = () => { let v = null; return { get: () => v, set: (x) => (v = x) }; };

test("completes the word being typed, most useful first", () => {
  const p = createPredictor({ store: memStore() });
  const s = p.suggest("how do i beat the bo");
  assert.equal(s[0], "boss", "after 'the', a game word beats plain frequency");
  assert.ok(s.length <= 4);
  assert.ok(s.filter((w) => !isEmoji(w)).every((w) => w.startsWith("bo")));
});

test("predicts the next word after a space", () => {
  const p = createPredictor({ store: memStore() });
  assert.ok(["i", "you"].includes(p.suggest("how do ")[0]), "how do -> i / you");
  assert.ok(p.suggest("where is ").includes("the"));
  assert.ok(p.suggest("").length > 0, "something to start with on an empty field");
});

test("learns the words and pairs the player sends", () => {
  const store = memStore();
  const p = createPredictor({ store });
  for (let i = 0; i < 3; i++) p.learn("malenia waterfowl dance");
  assert.equal(p.suggest("mal")[0], "malenia", "a new word is learned");
  assert.equal(p.suggest("malenia ")[0], "waterfowl", "and what follows it");
  const again = createPredictor({ store });
  assert.equal(again.suggest("mal")[0], "malenia", "kept between sessions");
});

test("forgives one typo when nothing starts that way", () => {
  const p = createPredictor({ store: memStore() });
  assert.ok(p.suggest("wepon").includes("weapon"));
});

test("keeps the player's capital letter", () => {
  const p = createPredictor({ store: memStore() });
  assert.ok(p.suggest("Wh").every((w) => isEmoji(w) || w.startsWith("Wh")));
});

test("suggests an emoji for a word that has one", () => {
  const p = createPredictor({ store: memStore() });
  assert.ok(p.suggest("that boss was fire").includes("🔥"));
  assert.ok(p.suggest("gg ").some(isEmoji));
  assert.ok(!p.suggest("where is the").some(isEmoji), "no emoji for plain words");
});

test("emoji the player uses most come first on the emoji page", () => {
  const p = createPredictor({ store: memStore() });
  const page = ["😂", "🔥", "👍", "💀"];
  p.learn("lol 💀 💀 💀");
  assert.deepEqual(p.emojiOrder(page).slice(0, 2), ["💀", "😂"]);
});
