import { test } from "node:test";
import assert from "node:assert/strict";
import { createCaptions } from "./live.js";

test("captions keep the current words bright", () => {
  const c = createCaptions();
  c.gemini("a b c d e f g h i j k l m n o");
  assert.equal(c.view().current, "d e f g h i j k l m n o");
  assert.equal(c.view().older, "a b c");
});

test("streamed pieces join, and a new turn clears the old one", () => {
  const c = createCaptions();
  c.gemini("It's behind");
  c.gemini(" the statue.");
  assert.equal(c.view().current, "It's behind the statue.");
  c.turnDone();
  assert.equal(c.view().current, "It's behind the statue.", "still shown after the turn");
  c.gemini("Next");
  assert.equal(c.view().current, "Next");
  assert.equal(c.view().older, "");
});

test("interrupted keeps what was said until the next turn", () => {
  const c = createCaptions();
  c.gemini("half a sen");
  c.interrupted();
  assert.equal(c.view().current, "half a sen");
  c.gemini("new answer");
  assert.equal(c.view().current, "new answer");
});

test("the player's line shows their latest words and starts over each turn", () => {
  const c = createCaptions();
  c.you("where is");
  c.you(" the lever");
  assert.equal(c.view().you, "where is the lever");
  c.you(" to open this gate in the sunken market please");
  assert.equal(c.view().you.split(" ").length, 10, "last ten words");
  c.gemini("Behind you.");
  c.turnDone();
  c.you("thanks");
  assert.equal(c.view().you, "thanks");
});
