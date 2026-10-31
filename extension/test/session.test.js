"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { conversationFromPath, cleanTitle, summonMessage, signalMessage, dismissMessage, listMessage } = require("../src/session.js");

test("reads the conversation id from the paths a conversation can have", () => {
  const id = "0a1b2c3d-1111-2222-3333-444455556666";
  assert.equal(conversationFromPath(`/c/${id}`), id);
  assert.equal(conversationFromPath(`/c/${id}/`), id);
  assert.equal(conversationFromPath(`/g/g-abc123-my-gpt/c/${id}`), id);
});

test("anywhere else on the site is not a conversation", () => {
  for (const path of ["/", "/gpts", "/c/", "/c/a/b", "/settings/c/abc", "/c/a b", "/c/../x", `/c/${"a".repeat(129)}`]) {
    assert.equal(conversationFromPath(path), null, path);
  }
});

test("the title loses the site's name and nothing else", () => {
  assert.equal(cleanTitle("Bug del login - ChatGPT"), "Bug del login");
  assert.equal(cleanTitle("ChatGPT - Bug del login"), "Bug del login");
  assert.equal(cleanTitle("Bug - ChatGPT - otra cosa"), "Bug - ChatGPT - otra cosa");
  assert.equal(cleanTitle("ChatGPT"), "");
  assert.equal(cleanTitle("  "), "");
});

test("a title can never carry what the host refuses", () => {
  assert.equal(cleanTitle("a\nb\u0000c"), "a b c");
  assert.equal(Array.from(cleanTitle("ñ".repeat(400))).length, 256);
});

test("messages carry exactly the fields the host accepts", () => {
  assert.deepEqual(listMessage(), { type: "list" });
  assert.deepEqual(summonMessage("mochi", "abc", "Bug - ChatGPT"), { type: "summon", pet: "mochi", conversation: "abc", name: "Bug" });
  assert.deepEqual(signalMessage("abc", "done"), { type: "signal", conversation: "abc", signal: "done" });
  assert.deepEqual(dismissMessage("mochi"), { type: "dismiss", pet: "mochi" });
});

test("a bad conversation or signal never becomes a message", () => {
  assert.throws(() => signalMessage("a/b", "done"));
  assert.throws(() => signalMessage("", "done"));
  assert.throws(() => signalMessage(null, "done"));
  assert.throws(() => signalMessage("abc", "exploded"));
  assert.throws(() => summonMessage("mochi", "../x", "n"));
});
