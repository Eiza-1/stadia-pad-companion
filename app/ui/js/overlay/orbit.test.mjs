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
