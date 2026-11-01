// Follows the conversation a tab is showing, and only reads its stop button if the user brought a
// pet to it. Plain logic: the page, the URL and the outside world are passed in.
(function (root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory(require("./session.js"), require("./detect.js"));
  } else {
    root.FamiliardWatcher = factory(root.FamiliardSession, root.FamiliardDetect);
  }
})(globalThis, function (session, detect) {
  "use strict";

  /**
   * @param doc the page, used only to look for a stop button
   * @param path gives the URL's path
   * @param isBound says whether the user brought a pet to a conversation
   * @param emit receives (conversation, "visit" | "working" | "done" | "registered")
   * @param rowSays reads a conversation's row: true, false, or null when it cannot say. It is
   *   believed over the stop button, which can stay on screen after moving between conversations
   */
  function createWatcher({ doc, path, isBound, emit, rowSays = () => null }) {
    let current = null;
    let tracker = null;
    let staleStop = false;

    /** The row when it can say. Otherwise the stop button, unless the row already disproved this
     *  very button, which then says nothing until it goes away. */
    function generating(conversation) {
      const row = rowSays(conversation);
      const stop = detect.isGenerating(doc);
      if (row === true || !stop) staleStop = false;
      else if (row === false) staleStop = true;
      return row ?? (stop && !staleStop);
    }

    /** A conversation out of sight may still be answering, or may finish unseen, so nothing here
     *  can say. "registered" only stops claiming it works: never "done", which would celebrate. */
    function leave() {
      if (current !== null && tracker.generating && rowSays(current) !== true) emit(current, "registered");
      current = null;
      tracker = null;
    }

    return {
      leave,

      /** Call it whenever something may have changed. */
      tick() {
        const conversation = session.conversationFromPath(path());
        if (conversation === null || !isBound(conversation)) {
          leave();
          return;
        }
        // Arriving starts afresh: a stop button that is not there yet says nothing about a turn
        // that ended meanwhile.
        if (conversation !== current) {
          leave();
          current = conversation;
          tracker = detect.createTracker();
          staleStop = false;
          emit(conversation, "visit");
        }
        const change = tracker.observe(generating(conversation));
        if (change) emit(conversation, change);
      },
    };
  }

  /** Runs at most once per `wait`, and always once more after the last call. */
  function throttle(run, wait, timers = globalThis) {
    let pending = null;
    let again = false;
    function fire() {
      pending = null;
      if (again) {
        again = false;
        run();
        pending = timers.setTimeout(fire, wait);
      }
    }
    return function () {
      if (pending !== null) {
        again = true;
        return;
      }
      run();
      pending = timers.setTimeout(fire, wait);
    };
  }

  return { createWatcher, throttle };
});
