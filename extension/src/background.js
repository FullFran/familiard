// Connects the hub to the browser. Firefox loads the other scripts from the manifest; a Chromium
// service worker imports them here.
/* global importScripts */
if (typeof importScripts === "function") importScripts("session.js", "hub.js");

(function () {
  "use strict";
  const api = globalThis.browser ?? globalThis.chrome;
  const HOST = "io.github.fullfran.familiard";

  const hub = globalThis.FamiliardHub.createHub({
    // A new host per message, gone once it has answered, so nothing stays running in between.
    native: (message) => api.runtime.sendNativeMessage(HOST, message),
    storage: {
      load: async () => (await api.storage.local.get("bound")).bound ?? {},
      save: (bound) => api.storage.local.set({ bound }),
    },
  });

  const fromPopup = (sender) => sender.url?.startsWith(api.runtime.getURL("popup/"));

  // Firefox gives a popup in a frame no `tabs`, so what it says to its own tab goes through here.
  const FOR_THE_TAB = new Set(["describe", "panelToken", "panelHeight"]);

  function handle(message, sender) {
    if (fromPopup(sender)) {
      if (message.type === "whereAmI") return Promise.resolve({ tab: sender.tab?.id });
      if (message.type === "toMyTab" && sender.tab && FOR_THE_TAB.has(message.message?.type)) {
        return api.tabs.sendMessage(sender.tab.id, message.message);
      }
      if (message.type === "status") return hub.status(message.conversation);
      if (message.type === "bring") return hub.bring(message);
      if (message.type === "dismiss") return hub.dismiss(message.conversation);
      if (message.type === "dismissPet") return hub.dismissPet(message.pet);
    }
    // A page may only report on what it shows, and the hub drops it without a pet.
    if (message.type === "report" && sender.tab) {
      const { conversation, signal } = message;
      return hub.report({ conversation, signal, tab: sender.tab.id });
    }
    return null;
  }

  api.runtime.onMessage.addListener((message, sender, reply) => {
    if (sender.id !== api.runtime.id) return undefined;
    const work = handle(message, sender);
    if (work === null) return undefined;
    work.then(
      (result) => reply(result ?? { ok: true }),
      (error) => reply({ ok: false, unavailable: true, error: String(error?.message ?? error) }),
    );
    return true;
  });

  api.tabs.onRemoved.addListener((tab) => hub.tabClosed(tab).catch(() => {}));
  api.runtime.onStartup.addListener(() => hub.browserStarted().catch(() => {}));
})();
