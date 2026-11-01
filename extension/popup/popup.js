// The only place a pet can come from: nothing appears unless the user presses the button here.
// Names come from other people's packs, so they only ever go in as text.
(function () {
  "use strict";
  const api = globalThis.browser ?? globalThis.chrome;
  const text = (key, ...values) => api.i18n.getMessage(key, values);
  const element = (id) => document.getElementById(id);
  // The tab this runs inside: none when opened from the browser's toolbar.
  let hostTab;

  function show(id, visible) {
    element(id).hidden = !visible;
  }

  function say(message) {
    element("message").textContent = message;
    show("message", Boolean(message));
  }

  /** What a page in a tab says to that tab, and the toolbar's popup to the tab it was opened on. */
  function askTab(tab, message) {
    if (hostTab === undefined) return api.tabs.sendMessage(tab, message);
    return api.runtime.sendMessage({ type: "toMyTab", message });
  }

  async function activeTab() {
    if (hostTab !== undefined) return hostTab;
    const [tab] = await api.tabs.query({ active: true, currentWindow: true });
    return tab?.id;
  }

  /** Which conversation the tab shows. A tab outside the site has no script to answer. */
  async function describe(tab) {
    if (tab === undefined) return null;
    try {
      const answer = await askTab(tab, { type: "describe" });
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

  async function dismissPet(pet) {
    const reply = await api.runtime.sendMessage({ type: "dismissPet", pet });
    await refresh(false);
    if (!reply.ok) say(explain(reply));
  }

  function fillList(section, list, rows) {
    element(list).replaceChildren();
    for (const { label, leave } of rows) {
      const item = document.createElement("li");
      const name = document.createElement("span");
      name.textContent = label;
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = text("dismissButton");
      button.addEventListener("click", leave);
      item.append(name, button);
      element(list).append(item);
    }
    show(section, rows.length > 0);
  }

  /** The pets out anywhere else, so that saying goodbye does not mean going there. */
  function fillOthers(others, loose) {
    fillList("others", "others-list", others.map((other) => ({
      label: other.name ? text("otherNamed", other.pet, other.name) : other.pet,
      leave: () => dismiss(other.conversation),
    })));
    fillList("loose", "loose-list", loose.map((pet) => ({
      label: pet.pet,
      leave: () => dismissPet(pet.pet),
    })));
  }

  async function refresh(keepName) {
    show("bring", false);
    show("bound", false);
    show("others", false);
    show("loose", false);
    say("");
    tab = await activeTab();
    const described = await describe(tab);
    conversation = described?.conversation ?? null;
    const status = await api.runtime.sendMessage({ type: "status", conversation });
    if (!status.ok) {
      say(explain(status));
      return;
    }
    fillOthers(status.others, status.loose);
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

  /** A page can open this popup by itself, in a frame or a tab. Only its own panel is shown. */
  async function recognised() {
    try {
      const token = location.hash.slice(1);
      const answer = await askTab(hostTab, { type: "panelToken", token });
      return answer?.known === true;
    } catch {
      return false;
    }
  }

  function followHeight() {
    const report = () => {
      const height = Math.ceil(document.documentElement.getBoundingClientRect().height);
      askTab(hostTab, { type: "panelHeight", height }).catch(() => {});
    };
    new ResizeObserver(report).observe(document.body);
  }

  async function start() {
    hostTab = (await api.runtime.sendMessage({ type: "whereAmI" }))?.tab;
    if (hostTab !== undefined) {
      if (!(await recognised())) return;
      followHeight();
    }
    await refresh(false);
  }
  start().catch((error) => say(String(error?.message ?? error)));
})();
