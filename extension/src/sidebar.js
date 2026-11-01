// A second source of signals: what the sidebar says about a conversation. It reports only a
// conversation the user brought a pet to, and reads only the state attribute of that row.
(function (root, factory) {
  if (typeof module === "object" && module.exports) module.exports = factory(require("./detect.js"));
  else root.FamiliardSidebar = factory(root.FamiliardDetect);
})(globalThis, function (detect) {
  "use strict";

  // Seen on the real page: a row gains data-sidebar-indicator="working" while it generates, and
  // "unread" when it finishes out of sight. An idle row has no such attribute at all.
  const ROW = "[data-sidebar-chatgpt-conversation-key]";
  const INDICATOR = "[data-sidebar-indicator]";
  const WORKING = '[data-sidebar-indicator="working"]';

  /**
   * Reads rows from a page. isBusy gives true or false when the row says, and null when nothing
   * can be said, which is never "finished": the row may be scrolled out of the list, the sidebar
   * closed, or this an older sidebar.
   *
   * An idle row looks the same as a page where ChatGPT renamed the attribute, so a row is only
   * believed once this page has shown the attribute at all. Until then everything is null, and
   * the extension behaves as it did with the stop button alone.
   */
  function createRowReader(doc) {
    let attributeExists = false;
    return {
      isBusy(conversation) {
        attributeExists = attributeExists || doc.querySelector(INDICATOR) !== null;
        if (!attributeExists) return null;
        const row = doc.querySelector(`a[href$="/c/${conversation}"]`)?.closest(ROW);
        // A closed sidebar keeps its rows in the page, frozen in their last state and with no box.
        if (!row || row.getClientRects().length === 0) return null;
        return row.querySelector(WORKING) !== null;
      },
    };
  }

  /**
   * @param isBusy reads a row
   * @param conversations gives the conversations with a pet
   * @param open gives the conversation on screen, which its own watcher follows
   * @param emit receives (conversation, "working" | "done")
   */
  function createSidebarWatcher({ isBusy, conversations, open, emit }) {
    const trackers = new Map();
    return {
      tick() {
        const shown = open();
        for (const conversation of [...trackers.keys()]) {
          if (conversation === shown || !conversations().includes(conversation)) trackers.delete(conversation);
        }
        for (const conversation of conversations()) {
          if (conversation === shown) continue;
          const busy = isBusy(conversation);
          if (busy === null) continue;
          if (!trackers.has(conversation)) trackers.set(conversation, detect.createTracker());
          const change = trackers.get(conversation).observe(busy);
          if (change) emit(conversation, change);
        }
      },
    };
  }

  return { createRowReader, createSidebarWatcher };
});
