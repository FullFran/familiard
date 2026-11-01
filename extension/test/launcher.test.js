// Where the button on the page goes, and who may open the panel.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { buttonAt, panelAt, answer } = require("../src/launcher.js");

const VIEW = { width: 1600, height: 900 };
const BOX = { left: 400, top: 780, right: 1200, bottom: 860 };

test("the button sits beside the box to write in", () => {
  assert.deepEqual(buttonAt(BOX, VIEW), { left: 1208, top: 824 });
});

test("with no room beside it, the button sits on top of the box", () => {
  assert.deepEqual(buttonAt({ ...BOX, left: 0, right: 600 }, { width: 620, height: 900 }), { left: 564, top: 736 });
});

test("without the box, the button waits in a corner instead of disappearing", () => {
  assert.deepEqual(buttonAt(null, VIEW), { left: 1548, top: 776 });
});

test("the button never leaves the window", () => {
  assert.deepEqual(buttonAt({ left: 0, top: 0, right: 40, bottom: 10 }, { width: 80, height: 60 }), { left: 8, top: 8 });
  assert.deepEqual(buttonAt({ left: 0, top: 0, right: 40, bottom: 10 }, { width: 8, height: 8 }), { left: 0, top: 0 });
});

test("the panel opens above the button, lined up with its right edge", () => {
  assert.deepEqual(panelAt({ left: 1208, top: 824 }, { width: 304, height: 200 }, VIEW), { left: 940, top: 616 });
});

test("the panel stays inside the window", () => {
  assert.deepEqual(panelAt({ left: 10, top: 20 }, { width: 304, height: 200 }, VIEW), { left: 8, top: 8 });
});

test("only the frame holding the token of the open panel is recognised", () => {
  const panel = { token: "right" };
  assert.deepEqual(answer({ type: "panelToken", token: "right" }, panel), { known: true });
  assert.deepEqual(answer({ type: "panelToken", token: "forged" }, panel), { known: false });
  assert.deepEqual(answer({ type: "panelToken", token: "" }, { token: "" }), { known: false });
  assert.deepEqual(answer({ type: "panelToken" }, { token: null }), { known: false });
  assert.equal(answer({ type: "describe" }, panel), undefined);
});
