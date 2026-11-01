// A second source of signals: what the sidebar shows about conversations that are not open. It
// reports only a conversation the user brought a pet to, and looks only at that conversation's row.
(function (root, factory) {
  if (typeof module === "object" && module.exports) module.exports = factory(require("./detect.js"));
  else root.FamiliardSidebar = factory(root.FamiliardDetect);
})(globalThis, function (detect) {
  "use strict";

  // What a row shows while its conversation is generating. Empty until it has been seen on the
  // real page, and while it is empty this whole file does nothing.
  const BUSY_SELECTORS = [];

  /** The row of a conversation, in either of the sidebars ChatGPT is rolling out, or null. */
  function rowOf(doc, conversation) {
    const keyed = doc.querySelector(`[data-sidebar-chatgpt-conversation-key="${conversation}"]`);
    if (keyed) return keyed;
    const link = doc.querySelector(`a[href="/c/${conversation}"]`);
    return link ? link.parentElement ?? link : null;
  }

  /** true or false when the row says; null when nothing can be said, which is never "finished":
   *  the row may be scrolled out of the list, or the sidebar closed. */
  function isBusy(doc, conversation, selectors = BUSY_SELECTORS) {
    if (selectors.length === 0) return null;
    const row = rowOf(doc, conversation);
    if (!row) return null;
    return selectors.some((selector) => {
      try {
        return Boolean(row.querySelector(selector));
      } catch {
        return false;
      }
    });
  }

  /**
   * @param conversations gives the conversations with a pet
   * @param open gives the conversation on screen, whose stop button says more than its row
   * @param emit receives (conversation, "working" | "done")
   */
  function createSidebarWatcher({ doc, conversations, open, emit, selectors = BUSY_SELECTORS }) {
    const trackers = new Map();
    return {
      tick() {
        const shown = open();
        for (const conversation of [...trackers.keys()]) {
          if (conversation === shown || !conversations().includes(conversation)) trackers.delete(conversation);
        }
        for (const conversation of conversations()) {
          if (conversation === shown) continue;
          const busy = isBusy(doc, conversation, selectors);
          if (busy === null) continue;
          if (!trackers.has(conversation)) trackers.set(conversation, detect.createTracker());
          const change = trackers.get(conversation).observe(busy);
          if (change) emit(conversation, change);
        }
      },
    };
  }

  return { BUSY_SELECTORS, rowOf, isBusy, createSidebarWatcher };
});
