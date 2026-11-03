import { test } from "node:test";
import assert from "node:assert/strict";
import { createOrbit, suggest, PAGES } from "./orbit.js";

test("starts on letters at t; right glides, A types, B deletes, X spaces", () => {
  const o = createOrbit();
  assert.equal(o.state.page, "abc");
  assert.equal(o.state.focus, "t");
  o.input("right");
  o.input("right");
  assert.equal(o.state.focus, "v");
  o.input("A");
  assert.equal(o.text, "v");
  o.input("B");
  assert.equal(o.text, "");
  o.input("X");
  assert.equal(o.text, " ");
});

test("wraps around at both ends", () => {
  const o = createOrbit();
  while (o.state.focus !== "a") o.input("left");
  o.input("left");
  assert.equal(o.state.focus, "z");
  o.input("right");
  assert.equal(o.state.focus, "a");
});

test("Y takes the first suggestion for the word being typed", () => {
  const o = createOrbit();
  o.text = "how do i beat the wa";
  const first = suggest("wa")[0];
  assert.ok(first && first.startsWith("wa") && first.length > 2, "a real word for 'wa'");
  assert.equal(o.state.suggestions[0], first);
  o.input("Y");
  assert.equal(o.text, `how do i beat the ${first} `);
});

test("bumpers switch letters, numbers, emoji", () => {
  const o = createOrbit();
  o.input("RB");
  assert.equal(o.state.page, "123");
  o.input("RB");
  assert.equal(o.state.page, "emoji");
  o.input("RB");
  assert.equal(o.state.page, "abc", "wraps");
  o.input("LB");
  assert.equal(o.state.page, "emoji");
  assert.ok(PAGES.emoji.includes("🔥") && PAGES.emoji.includes("🎮"));
  assert.ok(PAGES["123"].includes("?"));
});

test("L3 toggles caps for letters", () => {
  const o = createOrbit();
  o.input("L3");
  o.input("A");
  assert.equal(o.text, "T");
  o.input("L3");
  o.input("A");
  assert.equal(o.text, "Tt");
});

test("emoji count as one character for B", () => {
  const o = createOrbit();
  o.text = "gg 🔥";
  o.input("B");
  assert.equal(o.text, "gg ");
});

test("Menu sends the text", () => {
  const o = createOrbit();
  o.text = "combo?";
  assert.deepEqual(o.input("MENU"), { send: "combo?" });
});

test("suggest is case-insensitive, prefix-based and limited", () => {
  assert.ok(suggest("Bo").every((w) => w.startsWith("bo")));
  assert.ok(suggest("b").length <= 4);
  assert.deepEqual(suggest(""), []);
});

test("D-pad down and up choose among suggestions; Y takes the chosen one", () => {
  const o = createOrbit();
  o.text = "the wa";
  const s = o.state.suggestions;
  assert.ok(s.length >= 3, "several words for 'wa'");
  assert.equal(o.state.selected, 0);
  o.input("down");
  o.input("down");
  assert.equal(o.state.selected, 2);
  o.input("up");
  assert.equal(o.state.selected, 1);
  o.input("Y");
  assert.equal(o.text, `the ${s[1]} `);
});

test("the choice stays within the list and resets when the word changes", () => {
  const o = createOrbit();
  o.text = "wa";
  const n = o.state.suggestions.length;
  for (let i = 0; i < 10; i++) o.input("down");
  assert.equal(o.state.selected, n - 1);
  for (let i = 0; i < 10; i++) o.input("up");
  assert.equal(o.state.selected, 0);
  o.input("down");
  o.input("A");
  assert.equal(o.state.selected, 0, "typing resets the choice");
  o.input("down");
  o.text = "wa";
  assert.equal(o.state.selected, 0, "new text from the field resets the choice");
});

test("pick(i) takes a clicked suggestion", () => {
  const o = createOrbit();
  o.text = "the wa";
  const s = o.state.suggestions;
  o.pick(2);
  assert.equal(o.text, `the ${s[2]} `);
  o.pick(9); // nothing there: no change
  assert.equal(o.text, `the ${s[2]} `);
});
