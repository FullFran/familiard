// The only place a pet can come from: nothing appears unless the user presses the button here.
// Names come from other people's packs, so they only ever go in as text.
(function () {
  "use strict";
  const api = globalThis.browser ?? globalThis.chrome;
  const text = (key, ...values) => api.i18n.getMessage(key, values);
  const element = (id) => document.getElementById(id);

  function show(id, visible) {
    element(id).hidden = !visible;
  }

  function say(message) {
    element("message").textContent = message;
    show("message", Boolean(message));
  }

  async function activeTab() {
    const [tab] = await api.tabs.query({ active: true, currentWindow: true });
    return tab?.id;
  }

  /** Which conversation the tab shows. A tab outside the site has no script to answer. */
  async function describe(tab) {
    if (tab === undefined) return null;
    try {
      const answer = await api.tabs.sendMessage(tab, { type: "describe" });
      return answer?.conversation ? answer : null;
    } catch {
      return null;
    }
  }

  function fillPicker(pets) {
    const picker = element("pet");
    picker.replaceChildren();
    for (const pet of pets) {
      const option = document.createElement("option");
      option.value = pet.id;
      option.textContent = pet.open ? text("petOut", pet.name) : pet.name;
      option.disabled = pet.open;
      picker.append(option);
    }
    const free = pets.filter((pet) => !pet.open);
    picker.value = (free.find((pet) => pet.default) ?? free[0])?.id ?? "";
    return free.length;
  }

  function explain(reply) {
    return reply.unavailable ? text("hostUnavailable") : reply.error;
  }

  let conversation = null;
  let tab;

  async function dismiss(which) {
    const reply = await api.runtime.sendMessage({ type: "dismiss", conversation: which });
    await refresh(false);
    if (!reply.ok) say(explain(reply));
  }

  /** The pets out on other conversations, so that saying goodbye does not mean going there. */
  function fillOthers(others) {
    const list = element("others-list");
    list.replaceChildren();
    for (const other of others) {
      const item = document.createElement("li");
      const label = document.createElement("span");
      label.textContent = other.name ? text("otherNamed", other.pet, other.name) : other.pet;
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = text("dismissButton");
      button.addEventListener("click", () => dismiss(other.conversation));
      item.append(label, button);
      list.append(item);
    }
    show("others", others.length > 0);
  }

  async function refresh(keepName) {
    show("bring", false);
    show("bound", false);
    show("others", false);
    say("");
    tab = await activeTab();
    const described = await describe(tab);
    conversation = described?.conversation ?? null;
    const status = await api.runtime.sendMessage({ type: "status", conversation });
    if (!status.ok) {
      say(explain(status));
      return;
    }
    fillOthers(status.others);
    if (!described) {
      say(text("notAConversation"));
      return;
    }
    if (status.bound) {
      element("bound-text").textContent = status.bound.name
        ? text("boundNamed", status.bound.pet, status.bound.name)
        : text("bound", status.bound.pet);
      show("bound", true);
      return;
    }
    if (status.pets.length === 0) {
      say(text("emptyGallery"));
      return;
    }
    const free = fillPicker(status.pets);
    if (!keepName) element("name").value = described.title;
    element("bring-button").disabled = free === 0;
    if (free === 0) say(text("allOut"));
    show("bring", true);
  }

  element("bring").addEventListener("submit", async (event) => {
    event.preventDefault();
    element("bring-button").disabled = true;
    const reply = await api.runtime.sendMessage({
      type: "bring",
      pet: element("pet").value,
      conversation,
      name: element("name").value,
      tab,
    });
    await refresh(!reply.ok);
    if (!reply.ok) say(explain(reply));
  });

  element("dismiss-button").addEventListener("click", () => dismiss(conversation));

  for (const node of document.querySelectorAll("[data-i18n]")) {
    node.textContent = text(node.dataset.i18n);
  }
  refresh(false).catch((error) => say(String(error?.message ?? error)));
})();
