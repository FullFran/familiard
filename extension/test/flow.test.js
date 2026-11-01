// A page, the watcher and the hub working together, against a recording stand-in for the host.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { createWatcher, throttle } = require("../src/watcher.js");
const { createHub } = require("../src/hub.js");
const { fakeDocument, page, STOP } = require("./helpers.js");

const MINE = "aaaaaaaa-1111-2222-3333-444444444444";
const OTHER = "bbbbbbbb-1111-2222-3333-444444444444";

function world({ pets } = {}) {
  const sent = [];
  let stored = {};
  const open = new Set();
  const native = async (message) => {
    sent.push(message);
    if (message.type === "list") {
      return { ok: true, pets: (pets ?? [{ id: "mochi", name: "Mochi", default: true, open: false }]).map((p) => ({ ...p, open: open.has(p.id) })) };
    }
    if (message.type === "summon") {
      if (open.has(message.pet)) return { ok: false, already_out: true, error: "already out" };
      open.add(message.pet);
    }
    if (message.type === "dismiss") open.delete(message.pet);
    return { ok: true };
  };
  const storage = { load: async () => structuredClone(stored), save: async (next) => { stored = structuredClone(next); } };
  const hub = createHub({ native, storage });
  return { sent, hub, open, bound: () => stored };
}

/** A tab showing a page, connected as content.js connects it. */
function tab(w, id, startingAt) {
  const doc = fakeDocument(page());
  let path = `/c/${startingAt}`;
  const reports = [];
  const watcher = createWatcher({
    doc,
    path: () => path,
    isBound: (conversation) => Object.hasOwn(w.bound(), conversation),
    emit: (conversation, signal) => reports.push(w.hub.report({ conversation, signal, tab: id })),
  });
  return {
    async show(markup) {
      doc.replace(page(markup));
      watcher.tick();
      await Promise.all(reports);
    },
    async go(conversation, markup = "") {
      path = conversation ? `/c/${conversation}` : "/";
      await this.show(markup);
    },
  };
}

const signals = (w) => w.sent.filter((m) => m.type === "signal").map((m) => `${m.conversation.slice(0, 1)}:${m.signal}`);

test("nothing is sent and nothing appears until the user presses the button", async () => {
  const w = world();
  const page = tab(w, 1, MINE);
  await page.show(STOP);
  await page.show("");
  await page.show(STOP);
  assert.deepEqual(w.sent, []);
  assert.equal(w.open.size, 0);
});

test("a stop button appearing is working, and its going is done", async () => {
  const w = world();
  const page = tab(w, 1, MINE);
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await page.show("");
  await page.show(STOP);
  await page.show(STOP);
  await page.show("");
  await page.show("");
  assert.deepEqual(signals(w), ["a:working", "a:done"]);
});

test("a pet brought in the middle of a turn notices it is working", async () => {
  const w = world();
  const page = tab(w, 1, MINE);
  await page.show(STOP);
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await page.show(STOP);
  assert.deepEqual(signals(w), ["a:working"]);
});

test("a page whose selectors stopped matching reports nothing rather than guessing", async () => {
  const w = world();
  const page = tab(w, 1, MINE);
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await page.show('<button data-testid="renamed-stop-button"></button>');
  await page.show("");
  assert.deepEqual(signals(w), []);
});

test("leaving a conversation mid-answer stops claiming it works, without saying it is done", async () => {
  const w = world();
  const page = tab(w, 1, MINE);
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await page.show(STOP);
  await page.go(OTHER, STOP);
  assert.deepEqual(signals(w), ["a:working", "a:registered"], "it can no longer see that answer");
  await page.go(OTHER, "");
  await page.go(null);
  assert.deepEqual(signals(w), ["a:working", "a:registered"], "another conversation sends nothing");
  await page.go(MINE, STOP);
  assert.deepEqual(signals(w), ["a:working", "a:registered", "a:working"]);
  await page.show("");
  assert.deepEqual(signals(w), ["a:working", "a:registered", "a:working", "a:done"]);
});

test("leaving mid-answer stays silent if the sidebar can still see that answer", async () => {
  const doc = fakeDocument(page());
  let path = `/c/${MINE}`;
  const sent = [];
  const watcher = createWatcher({
    doc,
    path: () => path,
    isBound: () => true,
    stillVisible: (conversation) => conversation === MINE,
    emit: (conversation, signal) => sent.push(signal),
  });
  watcher.tick();
  doc.replace(page(STOP));
  watcher.tick();
  path = "/";
  doc.replace(page());
  watcher.tick();
  assert.deepEqual(sent, ["visit", "working"]);
});

test("leaving a conversation that is not answering sends nothing", async () => {
  const w = world();
  const page = tab(w, 1, MINE);
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await page.show(STOP);
  await page.show("");
  await page.go(OTHER, "");
  assert.deepEqual(signals(w), ["a:working", "a:done"]);
});

test("leaving for a page that is no conversation also stops claiming it works", async () => {
  const w = world();
  const page = tab(w, 1, MINE);
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await page.show(STOP);
  await page.go(null);
  assert.deepEqual(signals(w), ["a:working", "a:registered"]);
});

test("closing the tab ends the session once, and only the last tab counts", async () => {
  const w = world();
  const first = tab(w, 1, MINE);
  const second = tab(w, 2, MINE);
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await first.show("");
  await second.show("");
  await w.hub.tabClosed(1);
  assert.deepEqual(signals(w), []);
  await w.hub.tabClosed(2);
  assert.deepEqual(signals(w), ["a:ended"]);
  await w.hub.tabClosed(2);
  await w.hub.tabClosed(3);
  assert.deepEqual(signals(w), ["a:ended"]);
  assert.deepEqual(w.bound(), {});
});

test("closing a tab with no pet sends nothing", async () => {
  const w = world();
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await w.hub.tabClosed(9);
  assert.deepEqual(signals(w), []);
});

test("dismissing ends the session, sends the pet away and forgets the binding", async () => {
  const w = world();
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await w.hub.dismiss(MINE);
  assert.deepEqual(w.sent.slice(1), [
    { type: "signal", conversation: MINE, signal: "ended" },
    { type: "dismiss", pet: "mochi" },
  ]);
  assert.deepEqual(w.bound(), {});
  const before = w.sent.length;
  await w.hub.dismiss(MINE);
  assert.equal(w.sent.length, before, "nothing left to dismiss");
});

test("a pet the host refuses is not bound", async () => {
  const w = world();
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "One", tab: 1 });
  const reply = await w.hub.bring({ pet: "mochi", conversation: OTHER, name: "Two", tab: 1 });
  assert.equal(reply.ok, false);
  assert.deepEqual(Object.keys(w.bound()), [MINE]);
});

test("a conversation holds one pet at a time", async () => {
  const w = world({ pets: [{ id: "a", name: "A" }, { id: "b", name: "B" }] });
  await w.hub.bring({ pet: "a", conversation: MINE, name: "", tab: 1 });
  const reply = await w.hub.bring({ pet: "b", conversation: MINE, name: "", tab: 1 });
  assert.equal(reply.ok, false);
  assert.equal(w.open.has("b"), false);
});

test("the status shows the binding, and forgets a pet that left by other means", async () => {
  const w = world();
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  let status = await w.hub.status(MINE);
  assert.deepEqual(status.bound, { pet: "mochi", name: "Bug" });
  assert.equal((await w.hub.status(OTHER)).bound, null);
  w.open.delete("mochi");
  status = await w.hub.status(MINE);
  assert.equal(status.bound, null);
  assert.deepEqual(w.bound(), {});
});

test("any standard pack from the listing goes through, whatever its id or name", async () => {
  const pets = [
    { id: "codex-v1", name: "Versión Uno", default: true },
    { id: "codex_v2", name: "<b>Dos</b>", default: false },
  ];
  const w = world({ pets });
  const status = await w.hub.status(MINE);
  assert.deepEqual(status.pets.map((p) => p.id), ["codex-v1", "codex_v2"]);
  await w.hub.bring({ pet: "codex_v2", conversation: MINE, name: "", tab: 1 });
  assert.equal(w.open.has("codex_v2"), true);
});

test("a new browser session ends what the last one left behind", async () => {
  const w = world();
  await w.hub.bring({ pet: "mochi", conversation: MINE, name: "Bug", tab: 1 });
  await w.hub.browserStarted();
  assert.deepEqual(signals(w), ["a:ended"]);
  assert.deepEqual(w.bound(), {});
});

test("changes made together do not overwrite each other", async () => {
  const w = world({ pets: [{ id: "a", name: "A" }, { id: "b", name: "B" }] });
  await Promise.all([
    w.hub.bring({ pet: "a", conversation: MINE, name: "", tab: 1 }),
    w.hub.bring({ pet: "b", conversation: OTHER, name: "", tab: 2 }),
  ]);
  assert.deepEqual(Object.keys(w.bound()).sort(), [MINE, OTHER].sort());
});

test("the throttle runs at once, holds the rest, and always runs once more at the end", () => {
  const scheduled = [];
  const timers = { setTimeout: (run) => scheduled.push(run) };
  let runs = 0;
  const call = throttle(() => { runs += 1; }, 500, timers);
  call();
  assert.equal(runs, 1);
  call();
  call();
  assert.equal(runs, 1, "held");
  scheduled.shift()();
  assert.equal(runs, 2, "the trailing run");
  scheduled.shift()();
  assert.equal(runs, 2, "nothing more was asked");
  call();
  assert.equal(runs, 3, "free again");
});
