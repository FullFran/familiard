// What a conversation is called, and every message that may leave for the native host. Nothing
// else in the extension builds a message, so what can leave is all written here.
(function (root, factory) {
  if (typeof module === "object" && module.exports) module.exports = factory();
  else root.FamiliardSession = factory();
})(globalThis, function () {
  "use strict";

  const CONVERSATION_PATH = /^\/(?:g\/[^/]+\/)?c\/([A-Za-z0-9_-]{1,128})\/?$/;
  const VALID_CONVERSATION = /^[A-Za-z0-9_-]{1,128}$/;
  const SITE_NAME = "ChatGPT";
  const MAX_NAME = 256;
  const SIGNALS = ["working", "done", "ended"];

  /** The id in a conversation's URL path, or null anywhere else on the site. */
  function conversationFromPath(pathname) {
    const match = CONVERSATION_PATH.exec(pathname);
    return match ? match[1] : null;
  }

  /** A tab's title without the site's name. Empty when it says nothing else. */
  function cleanTitle(title) {
    const clean = String(title)
      .replace(new RegExp(`^${SITE_NAME}\\s*[-|–—]\\s*`), "")
      .replace(new RegExp(`\\s*[-|–—]\\s*${SITE_NAME}$`), "")
      // eslint-disable-next-line no-control-regex
      .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
      .trim();
    return clean === SITE_NAME ? "" : Array.from(clean).slice(0, MAX_NAME).join("");
  }

  function checkConversation(conversation) {
    if (typeof conversation !== "string" || !VALID_CONVERSATION.test(conversation)) {
      throw new Error("invalid conversation id");
    }
    return conversation;
  }

  function listMessage() {
    return { type: "list" };
  }

  function summonMessage(pet, conversation, name) {
    return { type: "summon", pet: String(pet), conversation: checkConversation(conversation), name: cleanTitle(name) };
  }

  function signalMessage(conversation, signal) {
    if (!SIGNALS.includes(signal)) throw new Error("unknown signal");
    return { type: "signal", conversation: checkConversation(conversation), signal };
  }

  function dismissMessage(pet) {
    return { type: "dismiss", pet: String(pet) };
  }

  return { conversationFromPath, cleanTitle, listMessage, summonMessage, signalMessage, dismissMessage };
});
