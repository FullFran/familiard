// One manifest for both browser families, and the ids it shares with the Rust side.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const ROOT = path.join(__dirname, "..");
const manifest = JSON.parse(fs.readFileSync(path.join(ROOT, "manifest.json"), "utf8"));
const installer = fs.readFileSync(path.join(ROOT, "../crates/cli/src/browser.rs"), "utf8");
const background = fs.readFileSync(path.join(ROOT, "src/background.js"), "utf8");
const constant = (name) => new RegExp(`const ${name}: &str = "([^"]+)"`).exec(installer)[1];

/** Chromium's id: the first 16 bytes of the key's SHA-256, each hex digit written as a-p. */
function chromiumId(key) {
  const hash = crypto.createHash("sha256").update(Buffer.from(key, "base64")).digest("hex").slice(0, 32);
  return [...hash].map((digit) => String.fromCharCode(97 + parseInt(digit, 16))).join("");
}

test("it is Manifest V3 with the background each family reads", () => {
  assert.equal(manifest.manifest_version, 3);
  assert.equal(manifest.background.service_worker, "src/background.js");
  assert.ok(manifest.background.scripts.at(-1) === "src/background.js");
  assert.equal(manifest.background.scripts.includes("src/hub.js"), true);
  assert.match(background, /importScripts\("session\.js", "hub\.js"\)/);
});

test("it states the first versions that accept both background keys", () => {
  assert.equal(manifest.minimum_chrome_version, "121");
  assert.equal(manifest.browser_specific_settings.gecko.strict_min_version, "142.0");
});

test("the installer and the manifest agree on both extension ids", () => {
  assert.equal(chromiumId(manifest.key), constant("CHROMIUM_ID"));
  assert.equal(manifest.browser_specific_settings.gecko.id, constant("GECKO_ID"));
});

test("the extension calls the host the installer registers", () => {
  assert.ok(background.includes(`"${constant("HOST_NAME")}"`));
});

test("every file the manifest names exists", () => {
  const files = [
    manifest.action.default_popup,
    manifest.background.service_worker,
    ...manifest.background.scripts,
    ...manifest.content_scripts.flatMap((script) => script.js),
    ...manifest.web_accessible_resources.flatMap((entry) => entry.resources),
  ];
  for (const file of files) assert.ok(fs.existsSync(path.join(ROOT, file)), file);
});

test("English is the default, and every message exists in Spanish too", () => {
  assert.equal(manifest.default_locale, "en");
  const messages = (locale) => JSON.parse(fs.readFileSync(path.join(ROOT, "_locales", locale, "messages.json"), "utf8"));
  const [en, es] = [messages("en"), messages("es")];
  assert.deepEqual(Object.keys(es).sort(), Object.keys(en).sort());
  assert.equal(en.bringButton.message, "Bring a waifu to this conversation");
  assert.equal(es.bringButton.message, "traer waifu a esta conversación");
  assert.equal(es.dismissButton.message, "despedir");
});

test("everything the popup shows has a message", () => {
  const en = JSON.parse(fs.readFileSync(path.join(ROOT, "_locales/en/messages.json"), "utf8"));
  const html = fs.readFileSync(path.join(ROOT, "popup/popup.html"), "utf8");
  const script = fs.readFileSync(path.join(ROOT, "popup/popup.js"), "utf8");
  const used = [...html.matchAll(/data-i18n="(\w+)"/g), ...script.matchAll(/text\("(\w+)"/g)].map((match) => match[1]);
  assert.ok(used.length > 5);
  for (const key of used) assert.ok(en[key], key);
});
