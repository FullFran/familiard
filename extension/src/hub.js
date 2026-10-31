// Which conversation has which pet, and what to tell the native host about it. Plain logic: the
// host and the storage are passed in. Nothing reaches the host for a conversation without a pet.
(function (root, factory) {
  if (typeof module === "object" && module.exports) module.exports = factory(require("./session.js"));
  else root.FamiliardHub = factory(root.FamiliardSession);
})(globalThis, function (session) {
  "use strict";

  /**
   * @param native sends one message to the host and resolves with its reply
   * @param storage `load()` and `save(bound)`, where bound is
   *        { [conversation]: { pet, name, tabs: [tabId] } }
   */
  function createHub({ native, storage }) {
    // One at a time: a worker can be stopped at any moment, so all state lives in storage, and
    // two changes running together would each overwrite the other.
    let queue = Promise.resolve();
    function serial(work) {
      const result = queue.then(work);
      queue = result.catch(() => {});
      return result;
    }

    async function end(conversation) {
      await native(session.signalMessage(conversation, "ended"));
    }

    return {
      /** The gallery, what this conversation has, and which pets went away by other means. */
      status(conversation) {
        return serial(async () => {
          const reply = await native(session.listMessage());
          if (!reply.ok) return reply;
          const open = new Set(reply.pets.filter((pet) => pet.open).map((pet) => pet.id));
          const bound = await storage.load();
          let changed = false;
          for (const [key, binding] of Object.entries(bound)) {
            if (!open.has(binding.pet)) {
              delete bound[key];
              changed = true;
            }
          }
          if (changed) await storage.save(bound);
          const mine = conversation && Object.hasOwn(bound, conversation) ? bound[conversation] : null;
          return { ok: true, pets: reply.pets, bound: mine && { pet: mine.pet, name: mine.name } };
        });
      },

      /** Only ever called because the user pressed the button. */
      bring({ pet, conversation, name, tab }) {
        return serial(async () => {
          const message = session.summonMessage(pet, conversation, name);
          const bound = await storage.load();
          if (Object.hasOwn(bound, conversation)) return { ok: false, error: "this conversation has a pet" };
          const reply = await native(message);
          if (!reply.ok) return reply;
          bound[conversation] = { pet: message.pet, name: message.name, tabs: tab === undefined ? [] : [tab] };
          await storage.save(bound);
          return reply;
        });
      },

      /** Says goodbye to the session too, so that pets following everything forget it. */
      dismiss(conversation) {
        return serial(async () => {
          const bound = await storage.load();
          if (!Object.hasOwn(bound, conversation)) return { ok: true };
          await end(conversation);
          const reply = await native(session.dismissMessage(bound[conversation].pet));
          delete bound[conversation];
          await storage.save(bound);
          return reply;
        });
      },

      /** What a page reports. `visit` tells nobody: it only records which tabs show what. */
      report({ conversation, signal, tab }) {
        return serial(async () => {
          const bound = await storage.load();
          if (!Object.hasOwn(bound, conversation)) return;
          const binding = bound[conversation];
          if (tab !== undefined && !binding.tabs.includes(tab)) {
            binding.tabs.push(tab);
            await storage.save(bound);
          }
          if (signal !== "visit") await native(session.signalMessage(conversation, signal));
        });
      },

      /** The last tab showing a conversation was closed: its session has ended. */
      tabClosed(tab) {
        return serial(async () => {
          const bound = await storage.load();
          let changed = false;
          for (const [conversation, binding] of Object.entries(bound)) {
            if (!binding.tabs.includes(tab)) continue;
            binding.tabs = binding.tabs.filter((other) => other !== tab);
            changed = true;
            if (binding.tabs.length === 0) {
              await end(conversation);
              delete bound[conversation];
            }
          }
          if (changed) await storage.save(bound);
        });
      },

      /** A new browser session has none of the tabs from the last one. */
      browserStarted() {
        return serial(async () => {
          const bound = await storage.load();
          for (const conversation of Object.keys(bound)) await end(conversation);
          await storage.save({});
        });
      },
    };
  }

  return { createHub };
});
