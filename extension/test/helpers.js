// A very small stand-in for a page, enough to run the selector table against fixture HTML without
// a DOM dependency. It understands exactly the selector shapes the table uses, and throws on any
// other, so a new shape in the table has to come with support here.
"use strict";
const fs = require("node:fs");
const path = require("node:path");

function parseElements(html) {
  const elements = [];
  const forms = [...html.matchAll(/<form\b[^>]*>.*?<\/form>/gs)].map((form) => [form.index, form.index + form[0].length]);
  for (const match of html.matchAll(/<([a-z][a-z0-9]*)((?:\s+[a-zA-Z-]+(?:="[^"]*")?)*)\s*\/?>/g)) {
    const attributes = new Map();
    for (const attribute of match[2].matchAll(/([a-zA-Z-]+)(?:="([^"]*)")?/g)) {
      attributes.set(attribute[1], attribute[2] ?? "");
    }
    const inForm = forms.some(([start, end]) => match.index > start && match.index < end);
    elements.push({ tag: match[1], attributes, inForm, hasAttribute: (name) => attributes.has(name) });
  }
  return elements;
}

function fakeDocument(html) {
  let elements = parseElements(html);
  return {
    querySelector(selector) {
      const match = /^(form )?([a-z]*)\[([a-z-]+)="([^"]*)"\]$/.exec(selector);
      if (!match) throw new Error(`unsupported selector ${selector}`);
      const [, form, tag, name, value] = match;
      return (
        elements.find((e) => (!form || e.inForm) && (!tag || e.tag === tag) && e.attributes.get(name) === value) ?? null
      );
    },
    replace(next) {
      elements = parseElements(next);
    },
  };
}

function fixture(name) {
  return fs.readFileSync(path.join(__dirname, "fixtures", name), "utf8");
}

const STOP = '<button data-testid="stop-button" aria-label="Stop">Stop</button>';

/** The fixture conversation, with the given markup where a stop button would go. */
function page(extra = "") {
  return fixture("conversation.html").replace("</form>", `${extra}</form>`);
}

module.exports = { fakeDocument, fixture, page, STOP };
