// The privacy rule in docs/browser.md, pinned: what a conversation says never leaves the page.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { createWatcher } = require("../src/watcher.js");
const { createHub } = require("../src/hub.js");
const { fakeDocument, fixture, page, STOP } = require("./helpers.js");

const ROOT = path.join(__dirname, "..");
const read = (file) => fs.readFileSync(path.join(ROOT, file), "utf8");
const SOURCES = ["src/session.js", "src/detect.js", "src/watcher.js", "src/content.js", "src/launcher.js", "src/hub.js", "src/background.js", "popup/popup.js"];
const MINE = "aaaaaaaa-1111-2222-3333-444444444444";
const OTHER = "bbbbbbbb-1111-2222-3333-444444444444";

test("the fixture really holds the text this is about", () => {
  for (const secret of ["SECRET-QUESTION", "SECRET-ANSWER", "SECRET-DRAFT"]) assert.ok(fixture("conversation.html").includes(secret));
});

test("a page's messages never appear in what is sent, and nothing at all for other conversations", async () => {
  const sent = [];
  let stored = {};
  const hub = createHub({
    native: async (message) => {
      sent.push(JSON.stringify(message));
      return message.type === "list" ? { ok: true, pets: [{ id: "mochi", name: "Mochi", open: true }] } : { ok: true };
    },
    storage: { load: async () => structuredClone(stored), save: async (next) => { stored = structuredClone(next); } },
  });
  const doc = fakeDocument(page());
  let where = `/c/${OTHER}`;
  const pending = [];
  const watcher = createWatcher({
    doc,
    path: () => where,
    isBound: (conversation) => Object.hasOwn(stored, conversation),
    emit: (conversation, signal) => pending.push(hub.report({ conversation, signal, tab: 1 })),
  });
  const show = async (markup) => {
    doc.replace(page(markup));
    watcher.tick();
    await Promise.all(pending);
  };

  await show(STOP);
  await show("");
  assert.deepEqual(sent, [], "no pet here, so nothing is sent");

  where = `/c/${MINE}`;
  await hub.bring({ pet: "mochi", conversation: MINE, name: "Bug del login - ChatGPT", tab: 1 });
  await hub.status(MINE);
  await show(STOP);
  await show("");
  await hub.tabClosed(1);

  const everything = sent.join("\n");
  assert.ok(everything.length > 0);
  assert.doesNotMatch(everything, /SECRET|hunter2/);
  assert.ok(!everything.includes(OTHER), "nothing about the other conversation");
  assert.ok(everything.includes("Bug del login"), "the title of the conversation the pet was brought to");
  const allowed = new Set(["type", "pet", "conversation", "name", "signal"]);
  for (const line of sent) {
    for (const key of Object.keys(JSON.parse(line))) assert.ok(allowed.has(key), key);
  }
});

test("the watcher looks at nothing but the stop button table", () => {
  const asked = [];
  const doc = {
    querySelector(selector) {
      asked.push(selector);
      return null;
    },
  };
  const watcher = createWatcher({ doc, path: () => `/c/${MINE}`, isBound: () => true, emit: () => {} });
  watcher.tick();
  assert.deepEqual(asked, require("../src/detect.js").STOP_SELECTORS);
  for (const selector of asked) assert.match(selector, /stop|detener/i);
});

test("a conversation without a pet does not even have its stop button looked at", () => {
  const doc = { querySelector() { throw new Error("should not be read"); } };
  const watcher = createWatcher({ doc, path: () => `/c/${OTHER}`, isBound: () => false, emit: assert.fail });
  watcher.tick();
});

test("no source reads page text, cookies or storage of the site, or uses the network", () => {
  const forbidden = [
    /\bfetch\s*\(/, /XMLHttpRequest/, /WebSocket/, /EventSource/, /sendBeacon/,
    /document\.cookie/, /localStorage/, /sessionStorage/, /indexedDB/,
    /innerText/, /innerHTML/, /outerHTML/, /textContent\b(?!\s*=)/, /webRequest/, /getSelection/,
    /characterData\s*:\s*true/,
  ];
  for (const file of SOURCES) {
    for (const pattern of forbidden) assert.doesNotMatch(read(file), pattern, `${file}: ${pattern}`);
  }
});

test("only the popup and the title suggestion touch document.title, and the content script only on request", () => {
  const titled = SOURCES.filter((file) => read(file).includes("document.title"));
  assert.deepEqual(titled, ["src/content.js"]);
  const content = read("src/content.js");
  assert.ok(content.indexOf('message?.type !== "describe"') < content.indexOf("document.title"), "after the popup asks");
});

test("the content script's messages hold only an id and a state", () => {
  const content = read("src/content.js");
  assert.match(content, /send\(\{ type: "report", conversation, signal \}\)/);
  assert.equal((content.match(/sendMessage/g) ?? []).length, 1);
});

test("the manifest asks for exactly what the docs say, on one site only", () => {
  const manifest = JSON.parse(read("manifest.json"));
  assert.deepEqual(manifest.permissions, ["nativeMessaging", "storage"]);
  assert.deepEqual(manifest.host_permissions, ["https://chatgpt.com/*"]);
  assert.deepEqual(manifest.content_scripts.map((s) => s.matches), [["https://chatgpt.com/*"]]);
  assert.equal(manifest.optional_permissions, undefined);
  assert.equal(manifest.externally_connectable, undefined);
  assert.deepEqual(manifest.web_accessible_resources, [
    { resources: ["popup/popup.html"], matches: ["https://chatgpt.com/*"] },
  ]);
});

test("the launcher asks the page only where the box to write in is, not what it says", () => {
  const launcher = read("src/launcher.js");
  assert.deepEqual([...launcher.matchAll(/querySelector\((\w+)\)/g)].map((match) => match[1]), ["COMPOSER"]);
  assert.ok(launcher.includes(`const COMPOSER = '#prompt-textarea, textarea[name="prompt"]';`));
  assert.doesNotMatch(launcher, /\.value\b|MutationObserver|getAttribute|\.dataset/);
  assert.equal((launcher.match(/sendMessage/g) ?? []).length, 0);
});

test("the panel lives where the page cannot reach it", () => {
  const launcher = read("src/launcher.js");
  assert.match(launcher, /attachShadow\(\{ mode: "closed" \}\)/);
  assert.match(launcher, /randomUUID\(\)/);
});
