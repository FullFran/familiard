// A button on the page that opens the popup right there. The panel is the popup itself, in a frame
// the page cannot read or click, so it stays the only place a pet can come from. All of it hangs
// from the root of the document, so ChatGPT's own tree is never touched, and the only thing asked
// of the page is where the box to write in is.
(function (root, factory) {
  if (typeof module === "object" && module.exports) module.exports = factory();
  else root.FamiliardLauncher = factory();
})(globalThis, function () {
  "use strict";
  const COMPOSER = '#prompt-textarea, textarea[name="prompt"]';
  const SIZE = 36;
  const GAP = 8;
  const CORNER = { right: 16, bottom: 88 };
  const PANEL_WIDTH = 304;
  const PLACE_EVERY_MS = 500;

  const STYLE = `
    button, iframe { position: fixed; z-index: 2147483647; box-sizing: border-box; }
    button {
      width: ${SIZE}px; height: ${SIZE}px; padding: 0; cursor: pointer; font: 18px/1 system-ui, sans-serif;
      border: 1px solid rgba(128, 128, 128, .4); border-radius: 50%; background: Canvas; color: CanvasText;
      color-scheme: light dark; box-shadow: 0 1px 4px rgba(0, 0, 0, .25);
    }
    iframe {
      box-sizing: content-box; width: ${PANEL_WIDTH}px; border: 1px solid rgba(128, 128, 128, .4); border-radius: 12px;
      box-shadow: 0 4px 16px rgba(0, 0, 0, .3);
    }
  `;

  /** Keeps something of this size inside the window, or at its corner if the window is smaller. */
  function inside(position, size, limit) {
    return Math.max(0, Math.min(Math.max(position, GAP), limit - size - GAP));
  }

  /** Beside the box to write in, on top of it with no room there, in a corner without it. */
  function buttonAt(box, view) {
    let left = view.width - SIZE - CORNER.right;
    let top = view.height - SIZE - CORNER.bottom;
    if (box) {
      const fits = box.right + GAP + SIZE + GAP <= view.width;
      left = fits ? box.right + GAP : box.right - SIZE;
      top = fits ? box.bottom - SIZE : box.top - SIZE - GAP;
    }
    return { left: inside(left, SIZE, view.width), top: inside(top, SIZE, view.height) };
  }

  /** Above the button, lined up with its right edge. */
  function panelAt(button, panel, view) {
    return {
      left: inside(button.left + SIZE - panel.width, panel.width, view.width),
      top: inside(button.top - GAP - panel.height, panel.height, view.height),
    };
  }

  /** A popup in a frame asks this before showing anything: a page can make a frame of its own. */
  function answer(message, panel) {
    if (message?.type !== "panelToken") return undefined;
    return { known: Boolean(panel.token) && message.token === panel.token };
  }

  function start({ api, doc, win }) {
    const host = doc.createElement("familiard-launcher");
    const shadow = host.attachShadow({ mode: "closed" });
    const style = doc.createElement("style");
    style.textContent = STYLE;
    const button = doc.createElement("button");
    button.type = "button";
    button.textContent = "🐾";
    button.title = api.i18n.getMessage("panelButton");
    button.setAttribute("aria-label", button.title);
    shadow.append(style, button);
    doc.documentElement.append(host);

    const panel = { token: null, frame: null, height: 200 };
    let at = { left: 0, top: 0 };
    const view = () => ({ width: win.innerWidth, height: win.innerHeight });

    function placePanel() {
      if (!panel.frame) return;
      const height = Math.min(panel.height, Math.floor(win.innerHeight * 0.8));
      const where = panelAt(at, { width: PANEL_WIDTH, height }, view());
      panel.frame.style.height = `${height}px`;
      panel.frame.style.left = `${where.left}px`;
      panel.frame.style.top = `${where.top}px`;
    }

    function place() {
      if (doc.hidden) return;
      const box = doc.querySelector(COMPOSER);
      const rect = (box?.closest("form") ?? box)?.getBoundingClientRect();
      // Something the page keeps hidden is no place to sit beside.
      at = buttonAt(rect?.width ? rect : null, view());
      button.style.left = `${at.left}px`;
      button.style.top = `${at.top}px`;
      placePanel();
    }

    function close() {
      panel.frame?.remove();
      panel.frame = null;
      panel.token = null;
    }

    function open() {
      panel.token = win.crypto.randomUUID();
      panel.frame = doc.createElement("iframe");
      panel.frame.src = `${api.runtime.getURL("popup/popup.html")}#${panel.token}`;
      panel.frame.title = button.title;
      shadow.append(panel.frame);
      placePanel();
    }

    button.addEventListener("click", () => (panel.frame ? close() : open()));
    doc.addEventListener("pointerdown", (event) => {
      if (panel.frame && !event.composedPath().includes(host)) close();
    }, true);
    win.addEventListener("resize", place);
    win.setInterval(place, PLACE_EVERY_MS);
    place();

    api.runtime.onMessage.addListener((message, sender, reply) => {
      if (sender.id !== api.runtime.id) return undefined;
      if (message?.type === "panelHeight") {
        panel.height = Number(message.height) || panel.height;
        placePanel();
        return undefined;
      }
      const known = answer(message, panel);
      if (known) reply(known);
      return undefined;
    });
  }

  return { buttonAt, panelAt, answer, start };
});
