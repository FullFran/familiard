// Runs on chatgpt.com. Connects the watcher to the page. It only watches the page at all once a
// pet has been brought to some conversation, and it never observes text changes.
(function () {
  "use strict";
  const api = globalThis.browser ?? globalThis.chrome;
  const { conversationFromPath, cleanTitle } = globalThis.FamiliardSession;
  const { createWatcher, throttle } = globalThis.FamiliardWatcher;
  const THROTTLE_MS = 500;
  // Attributes a stop button can come and go by. Deliberately not `characterData`.
  const OBSERVED_ATTRIBUTES = ["data-testid", "aria-label", "hidden"];

  let bound = {};
  const send = (message) => api.runtime.sendMessage(message).catch(() => {});

  const watcher = createWatcher({
    doc: document,
    path: () => location.pathname,
    isBound: (conversation) => Object.hasOwn(bound, conversation),
    emit: (conversation, signal) => send({ type: "report", conversation, signal }),
  });
  const tick = throttle(() => watcher.tick(), THROTTLE_MS);
  const observer = new MutationObserver(tick);
  let observing = false;

  function apply(next) {
    bound = next ?? {};
    const wanted = Object.keys(bound).length > 0;
    if (wanted && !observing) {
      observer.observe(document.documentElement, {
        subtree: true,
        childList: true,
        attributes: true,
        attributeFilter: OBSERVED_ATTRIBUTES,
      });
    } else if (!wanted && observing) {
      observer.disconnect();
    }
    observing = wanted;
    tick();
  }

  // Reloading or unloading the tab ends this script, and with it any news of the answer.
  addEventListener("pagehide", () => watcher.leave());

  api.storage.local.get("bound").then((stored) => apply(stored.bound));
  api.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && changes.bound) apply(changes.bound.newValue);
  });

  // The popup asking which conversation this is. The title goes no further than the popup,
  // which shows it as a suggestion until the user presses the button.
  api.runtime.onMessage.addListener((message, sender, reply) => {
    if (sender.id !== api.runtime.id || message?.type !== "describe") return undefined;
    const conversation = conversationFromPath(location.pathname);
    reply({ conversation, title: conversation ? cleanTitle(document.title) : "" });
    return undefined;
  });
})();
