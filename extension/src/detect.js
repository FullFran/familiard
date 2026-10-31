// How to tell that a conversation is generating: it shows a stop button. When ChatGPT changes its
// page this table is the only thing to update; see docs/browser.md.
(function (root, factory) {
  if (typeof module === "object" && module.exports) module.exports = factory();
  else root.FamiliardDetect = factory();
})(globalThis, function () {
  "use strict";

  const STOP_SELECTORS = [
    '[data-testid="stop-button"]',
    '[data-testid="fruitjuice-stop-button"]',
    'button[aria-label="Stop generating"]',
    'button[aria-label="Stop streaming"]',
  ];

  /** Whether a stop button is there. Looks at nothing else on the page. A selector that does
   *  not match, or that a browser refuses, is simply no signal. */
  function isGenerating(doc, selectors = STOP_SELECTORS) {
    return selectors.some((selector) => {
      try {
        const button = doc.querySelector(selector);
        return Boolean(button) && !button.hasAttribute("hidden");
      } catch {
        return false;
      }
    });
  }

  /** Turns "is there a stop button now" into what changed: appearing is working, and
   *  disappearing after having appeared is done. Never having seen it says nothing. */
  function createTracker() {
    let generating = false;
    return {
      observe(now) {
        if (now === generating) return null;
        generating = now;
        return now ? "working" : "done";
      },
    };
  }

  return { STOP_SELECTORS, isGenerating, createTracker };
});
