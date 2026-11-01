"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { createRowReader, createSidebarWatcher } = require("../src/sidebar.js");
const { createWatcher } = require("../src/watcher.js");

/**
 * The real row, reduced: a keyed list item whose key is not the conversation id, holding a link
 * and, only while there is news, a node with the indicator.
 * rows maps a conversation to "working", "unread" or null.
 */
function page(rows, options = {}) {
  const { keyed = true, stop = false } = options;
  const indicatorOf = (state) => (state ? { state } : null);
  const row = (conversation) => ({
    getClientRects: () => (options.closed ? [] : [{}]),
    querySelector: (selector) => {
      assert.equal(selector, '[data-sidebar-indicator="working"]');
      return rows[conversation] === "working" ? indicatorOf("working") : null;
    },
  });
  return {
    asked: [],
    querySelector(selector) {
      this.asked.push(selector);
      if (selector === "[data-sidebar-indicator]") {
        return Object.values(rows).some(Boolean) ? {} : null;
      }
      if (selector.includes("stop") || selector.includes("Stop") || selector.includes("Detener")) {
        return stop ? { hasAttribute: () => false } : null;
      }
      const match = /^a\[href\$="\/c\/(.+)"\]$/.exec(selector);
      assert.ok(match, `unexpected selector ${selector}`);
      if (!(match[1] in rows)) return null;
      return {
        closest: (wanted) => {
          assert.equal(wanted, "[data-sidebar-chatgpt-conversation-key]");
          return keyed ? row(match[1]) : null;
        },
      };
    },
  };
}

function watch(doc, { conversations = ["a"], open = "other" } = {}) {
  const sent = [];
  const reader = createRowReader(doc);
  const watcher = createSidebarWatcher({
    isBusy: reader.isBusy,
    conversations: () => conversations,
    open: () => open,
    emit: (conversation, signal) => sent.push(`${conversation}:${signal}`),
  });
  return { sent, watcher, reader };
}

test("the real sequence: working, then the unread dot, is done once", () => {
  const rows = { a: null };
  const { sent, watcher } = watch(page(rows));
  watcher.tick();
  assert.deepEqual(sent, [], "never having been busy says nothing");
  rows.a = "working";
  watcher.tick();
  watcher.tick();
  assert.deepEqual(sent, ["a:working"]);
  rows.a = "unread";
  watcher.tick();
  rows.a = null;
  watcher.tick();
  assert.deepEqual(sent, ["a:working", "a:done"]);
});

test("a row that disappears is unknown, not finished", () => {
  const rows = { a: "working" };
  const { sent, watcher } = watch(page(rows));
  watcher.tick();
  delete rows.a;
  rows.other = "working";
  watcher.tick();
  assert.deepEqual(sent, ["a:working"]);
  rows.a = "working";
  watcher.tick();
  assert.deepEqual(sent, ["a:working"], "back and still busy changes nothing");
  rows.a = null;
  watcher.tick();
  assert.deepEqual(sent, ["a:working", "a:done"]);
});

test("until the page has shown the attribute, an idle row says nothing", () => {
  const rows = { a: null };
  const { reader } = watch(page(rows));
  assert.equal(reader.isBusy("a"), null, "this could be a page where the attribute was renamed");
  rows.a = "working";
  assert.equal(reader.isBusy("a"), true);
  rows.a = null;
  assert.equal(reader.isBusy("a"), false, "once seen, its absence means idle");
});

test("a row of an older sidebar, with no key, says nothing", () => {
  const rows = { a: "working" };
  const { reader } = watch(page(rows, { keyed: false }));
  assert.equal(reader.isBusy("a"), null);
});

test("with the sidebar closed a row keeps its last state, which says nothing", () => {
  // Seen on the real page: closing it hides the rows with their state frozen, until it reopens.
  const rows = { a: "working" };
  const options = {};
  const { sent, watcher, reader } = watch(page(rows, options));
  watcher.tick();
  options.closed = true;
  assert.equal(reader.isBusy("a"), null);
  rows.a = null;
  watcher.tick();
  assert.deepEqual(sent, ["a:working"], "out of sight it cannot be known to have finished");
  options.closed = false;
  watcher.tick();
  assert.deepEqual(sent, ["a:working", "a:done"], "reopening tells");
});

test("a conversation without a pet does not even have its row looked at", () => {
  const doc = page({ a: "working", b: "working" });
  const { sent, watcher } = watch(doc, { conversations: ["a"] });
  watcher.tick();
  assert.deepEqual(sent, ["a:working"]);
  assert.ok(doc.asked.every((selector) => !selector.includes("/c/b")));
});

test("the conversation on screen is left to its own watcher", () => {
  const doc = page({ a: "working" });
  const { sent, watcher } = watch(doc, { open: "a" });
  watcher.tick();
  assert.deepEqual(sent, []);
  assert.deepEqual(doc.asked, []);
});

test("several conversations are followed apart", () => {
  const rows = { a: "working", b: null };
  const { sent, watcher } = watch(page(rows), { conversations: ["a", "b"] });
  watcher.tick();
  rows.b = "working";
  rows.a = "unread";
  watcher.tick();
  assert.deepEqual(sent, ["a:working", "a:done", "b:working"]);
});

/** The watcher of the open conversation, with the row reader as content.js wires it. */
function open(doc, shown, bound = ["a", "b"]) {
  const sent = [];
  const reader = createRowReader(doc);
  const watcher = createWatcher({
    doc,
    path: () => `/c/${shown.current}`,
    isBound: (conversation) => bound.includes(conversation),
    rowSays: reader.isBusy,
    emit: (conversation, signal) => sent.push(`${conversation}:${signal}`),
  });
  return { sent, watcher };
}

test("a stop button left over from another conversation does not make this one work", () => {
  // Seen on the real page: after moving from a generating A to an idle B, the stop button stayed
  // in B for as long as B was open, even a minute after A had finished.
  const rows = { a: "working", b: null };
  const doc = page(rows, { stop: true });
  const shown = { current: "b" };
  const { sent, watcher } = open(doc, shown);
  watcher.tick();
  rows.a = "unread";
  watcher.tick();
  assert.deepEqual(sent, ["b:visit"]);
});

test("the row of the open conversation drives it from working to done", () => {
  const rows = { a: "working", b: null };
  const doc = page(rows);
  const shown = { current: "a" };
  const { sent, watcher } = open(doc, shown);
  watcher.tick();
  rows.a = null;
  rows.b = "working";
  watcher.tick();
  assert.deepEqual(sent, ["a:visit", "a:working", "a:done"]);
});

test("without a readable row the open conversation still goes by its stop button", () => {
  const doc = page({}, { stop: true });
  const shown = { current: "a" };
  const { sent, watcher } = open(doc, shown);
  watcher.tick();
  assert.deepEqual(sent, ["a:visit", "a:working"]);
});

test("with the sidebar closed the open conversation goes by its stop button", () => {
  const rows = { a: "working", b: null };
  const doc = page(rows, { closed: true, stop: true });
  const shown = { current: "a" };
  const { sent, watcher } = open(doc, shown);
  watcher.tick();
  assert.deepEqual(sent, ["a:visit", "a:working"]);
  const done = page(rows, { closed: true, stop: false });
  doc.querySelector = done.querySelector.bind(done);
  watcher.tick();
  assert.deepEqual(sent, ["a:visit", "a:working", "a:done"], "even though the frozen row says working");
});

test("closing the sidebar does not revive a stop button the row already disproved", () => {
  const rows = { a: "working", b: null };
  const options = { stop: true };
  const shown = { current: "b" };
  const { sent, watcher } = open(page(rows, options), shown);
  watcher.tick();
  options.closed = true;
  watcher.tick();
  assert.deepEqual(sent, ["b:visit"]);
});

test("a stop button that left and came back counts again with the sidebar closed", () => {
  const rows = { a: "working", b: null };
  const doc = page(rows, { stop: true });
  const shown = { current: "b" };
  const { sent, watcher } = open(doc, shown);
  watcher.tick();
  const gone = page(rows, { closed: true, stop: false });
  doc.querySelector = gone.querySelector.bind(gone);
  watcher.tick();
  const back = page(rows, { closed: true, stop: true });
  doc.querySelector = back.querySelector.bind(back);
  watcher.tick();
  assert.deepEqual(sent, ["b:visit", "b:working"]);
});

test("a row that catches up with its stop button leaves it believed afterwards", () => {
  const rows = { a: null, b: "working" };
  const options = { stop: true };
  const shown = { current: "a" };
  const { sent, watcher } = open(page(rows, options), shown);
  watcher.tick();
  rows.a = "working";
  watcher.tick();
  options.closed = true;
  watcher.tick();
  assert.deepEqual(sent, ["a:visit", "a:working"], "closing the sidebar is not the answer ending");
});

test("leaving a generating conversation the sidebar can see does not stop it working", () => {
  const rows = { a: "working", b: null };
  const shown = { current: "a" };
  const { sent, watcher } = open(page(rows), shown);
  watcher.tick();
  shown.current = "b";
  watcher.tick();
  assert.deepEqual(sent, ["a:visit", "a:working", "b:visit"]);
});

test("leaving one the sidebar cannot see stops claiming it works", () => {
  const shown = { current: "a" };
  const { sent, watcher } = open(page({}, { stop: true }), shown);
  watcher.tick();
  shown.current = "b";
  watcher.tick();
  assert.equal(sent.includes("a:registered"), true);
});
