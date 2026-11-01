"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { BUSY_SELECTORS, isBusy, createSidebarWatcher } = require("../src/sidebar.js");

const SPINNER = ".spinner";

/** A page of rows, each one with or without a spinner, in either sidebar. */
function sidebar(rows, keyed = true) {
  const row = (conversation) => ({
    querySelector: (selector) => (selector === SPINNER && rows[conversation] ? {} : null),
  });
  return {
    asked: [],
    querySelector(selector) {
      this.asked.push(selector);
      const match = keyed
        ? /^\[data-sidebar-chatgpt-conversation-key="(.+)"\]$/.exec(selector)
        : /^a\[href="\/c\/(.+)"\]$/.exec(selector);
      if (!match || !(match[1] in rows)) return null;
      return keyed ? row(match[1]) : { parentElement: row(match[1]) };
    },
  };
}

function watch(doc, { conversations = ["a"], open = "other" } = {}) {
  const sent = [];
  const watcher = createSidebarWatcher({
    doc,
    conversations: () => conversations,
    open: () => open,
    emit: (conversation, signal) => sent.push(`${conversation}:${signal}`),
    selectors: [SPINNER],
  });
  return { sent, watcher };
}

test("with no selectors known it looks at nothing and says nothing", () => {
  assert.deepEqual(BUSY_SELECTORS, [], "the table stays empty until it is seen on the real page");
  const doc = sidebar({ a: true });
  assert.equal(isBusy(doc, "a"), null);
  assert.deepEqual(doc.asked, []);
});

for (const keyed of [true, false]) {
  const kind = keyed ? "keyed rows" : "link rows";

  test(`${kind}: a conversation out of sight goes from working to done`, () => {
    const rows = { a: false };
    const { sent, watcher } = watch(sidebar(rows, keyed));
    watcher.tick();
    assert.deepEqual(sent, [], "never having been busy says nothing");
    rows.a = true;
    watcher.tick();
    watcher.tick();
    assert.deepEqual(sent, ["a:working"]);
    rows.a = false;
    watcher.tick();
    assert.deepEqual(sent, ["a:working", "a:done"]);
  });

  test(`${kind}: a row that disappears is unknown, not finished`, () => {
    const rows = { a: true };
    const { sent, watcher } = watch(sidebar(rows, keyed));
    watcher.tick();
    delete rows.a;
    watcher.tick();
    assert.deepEqual(sent, ["a:working"]);
    rows.a = true;
    watcher.tick();
    assert.deepEqual(sent, ["a:working"], "back and still busy changes nothing");
    rows.a = false;
    watcher.tick();
    assert.deepEqual(sent, ["a:working", "a:done"]);
  });
}

test("a conversation without a pet does not even have its row looked at", () => {
  const doc = sidebar({ a: true, b: true });
  const { sent, watcher } = watch(doc, { conversations: ["a"] });
  watcher.tick();
  assert.deepEqual(sent, ["a:working"]);
  assert.ok(doc.asked.every((selector) => !selector.includes('"b"') && !selector.includes("/c/b")));
});

test("the conversation on screen is left to its stop button", () => {
  const doc = sidebar({ a: true });
  const { sent, watcher } = watch(doc, { open: "a" });
  watcher.tick();
  assert.deepEqual(sent, []);
  assert.deepEqual(doc.asked, []);
});

test("several conversations are followed apart", () => {
  const rows = { a: true, b: false };
  const { sent, watcher } = watch(sidebar(rows), { conversations: ["a", "b"] });
  watcher.tick();
  rows.b = true;
  rows.a = false;
  watcher.tick();
  assert.deepEqual(sent, ["a:working", "a:done", "b:working"]);
});

test("a selector the browser refuses is no signal", () => {
  const doc = { querySelector: () => ({ querySelector: () => { throw new Error("bad selector"); } }) };
  assert.equal(isBusy(doc, "a", ["::bad"]), false);
});
