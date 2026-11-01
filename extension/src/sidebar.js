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
  const WORKING = '[data-sidebar-indicator="working"]';

  /**
   * Reads rows from a page. isBusy gives true or false when the row says, and null when nothing
   * can be said, which is never "finished": the row may be scrolled out of the list, the sidebar
   * closed, or this an older sidebar.
   *
   * An idle row looks the same as a page where ChatGPT renamed the attribute or its value, so a
   * row is only believed once this page has shown a row working. Until then everything is null,
   * and the extension behaves as it did with the stop button alone.
   */
  function createRowReader(doc) {
    let stateIsKnown = false;
    return {
      isBusy(conversation) {
        stateIsKnown = stateIsKnown || doc.querySelector(WORKING) !== null;
        if (!stateIsKnown) return null;
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
   * @param emit receives (conversation, "working" | "done" | "registered")
   */
  function createSidebarWatcher({ isBusy, conversations, open, emit }) {
    const trackers = new Map();
    return {
      /** The page is going away, and with it any news of what these were answering. */
      leave() {
        for (const [conversation, tracker] of trackers) {
          if (tracker.generating) emit(conversation, "registered");
        }
        trackers.clear();
      },

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
