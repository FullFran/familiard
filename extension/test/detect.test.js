"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { STOP_SELECTORS, isGenerating, createTracker } = require("../src/detect.js");
const { fakeDocument, page, STOP } = require("./helpers.js");

test("each selector in the table recognises its stop button", () => {
  const buttons = [
    '<button data-testid="stop-button"></button>',
    '<button data-testid="fruitjuice-stop-button"></button>',
    '<button aria-label="Stop generating"></button>',
    '<button aria-label="Stop streaming"></button>',
  ];
  assert.equal(buttons.length, STOP_SELECTORS.length);
  for (const button of buttons) assert.equal(isGenerating(fakeDocument(page(button))), true, button);
});

test("the table starts with the test ids, before any label", () => {
  assert.deepEqual(STOP_SELECTORS.slice(0, 2), ['[data-testid="stop-button"]', '[data-testid="fruitjuice-stop-button"]']);
});

test("a page without a stop button is not generating", () => {
  assert.equal(isGenerating(fakeDocument(page())), false);
});

test("a hidden stop button does not count", () => {
  assert.equal(isGenerating(fakeDocument(page('<button data-testid="stop-button" hidden></button>'))), false);
});

test("selectors that match nothing, or that fail, are quietly no signal", () => {
  assert.equal(isGenerating(fakeDocument(page(STOP)), ['[data-testid="renamed"]']), false);
  assert.equal(isGenerating(fakeDocument(page(STOP)), ["::not a selector"]), false);
  const unwell = { querySelector() { throw new Error("boom"); } };
  assert.equal(isGenerating(unwell), false);
});

test("appearing is working, and disappearing after having appeared is done", () => {
  const tracker = createTracker();
  assert.equal(tracker.observe(true), "working");
  assert.equal(tracker.observe(true), null);
  assert.equal(tracker.observe(false), "done");
  assert.equal(tracker.observe(false), null);
  assert.equal(tracker.observe(true), "working");
});

test("never having seen a stop button says nothing, not even done", () => {
  const tracker = createTracker();
  assert.equal(tracker.observe(false), null);
  assert.equal(tracker.observe(false), null);
});
