# A pet for a ChatGPT conversation

ChatGPT on the web has no hooks, so a small browser extension watches the two things that say
whether a conversation is working: its row in the sidebar and its stop button. You press a button to bring a pet to the
conversation you are in. The pet takes the conversation's name, and tells you when it finishes.

**Not supported by OpenAI.** This reads a page that can change at any moment, so it may break
without notice. It has been tested on the real chatgpt.com in Spanish, in Zen on Linux. If it does
not react, see [When ChatGPT changes](#when-chatgpt-changes).

## Set it up

1. Connect the browsers to familiard. This writes one small file per browser, and needs running
   again if you move the binary:

   ```sh
   familiard browser install
   ```

2. Load the `extension/` folder of this repository:

   | Browser | How | Lasts |
   | --- | --- | --- |
   | Chromium, Chrome, Brave | `chrome://extensions`, turn on Developer mode, **Load unpacked** | Until you remove it |
   | Firefox, Zen | `about:debugging#/runtime/this-firefox`, **Load Temporary Add-on**, pick `manifest.json` | **Until the browser closes** |
   | Firefox Developer Edition, Nightly or ESR | Set `xpinstall.signatures.required` to `false` in `about:config`, then install a zip of the folder from `about:addons` | Until you remove it |

3. On Firefox and Zen, check that it may read chatgpt.com: **Manage Extension**, **Permissions**.
   Firefox can treat that permission as optional, and without it the button will not appear.
4. Reload any ChatGPT tab that was already open.

Firefox and Zen need a signed add-on to keep one installed after a restart. Each
[release](https://github.com/FullFran/familiard/releases) has one, signed by Mozilla but not listed
on its store: `familiard-extension-<version>-firefox.xpi`. Open the file with the browser to install
it. Chromium browsers use the `-chromium.zip` from the same release, unpacked, as above.

You need Firefox or Zen 142, or Chromium 121, or newer. Firefox 142 is the first to read this
add-on's statement that it collects no data, and Chromium 121 the first to accept one manifest for
both.

## Use it

1. Open a conversation and click the extension's button in the toolbar.
2. Choose a pet from your gallery. Any pet in `familiard pets` works, including any standard Codex
   pack. The name starts as the conversation's title; change it if you like.
3. Press **Bring a waifu to this conversation** (in Spanish, **traer waifu a esta conversación**).

No pet ever appears unless you press that button. Opening a conversation or sending a message does
nothing by itself.

| In the conversation | The pet |
| --- | --- |
| It starts answering | Works: `Bug del login · working` |
| It finishes, even if you moved to another conversation | Celebrates: `Bug del login · done` |
| You press **Dismiss** (**despedir**), or `familiard dismiss` | Leaves |
| You close the last tab showing it | Leaves |
| You restart the browser | Leaves when the browser opens again |

Things to know:

- A pet can only be out once, so a pet that is already out cannot be brought to a second
  conversation. For one pet per conversation, keep several in your gallery. `familiard add <folder>
  --as other-name` adds the same pack twice.
- A conversation has one pet at a time.
- **Keep the sidebar open to follow several conversations.** A closed sidebar stops updating its
  rows. The conversation on screen then goes by its stop button, and a conversation out of sight
  keeps working until you reopen the sidebar, which is when it celebrates. After moving between
  conversations the stop button can also be wrong, so with the sidebar closed a conversation you
  moved to may not start working.
- A tab only knows about the conversations answering in that tab. Another tab's answers do not
  reach its sidebar, nor a row scrolled out of the list.
- If a conversation's state cannot be known for 5 minutes its pet goes idle, but it stays.
- A pet with no binding, from a plain `familiard summon`, follows every session, and that includes
  these conversations.
- `chatgpt:` sessions report no folder, so they never claim a pet summoned with `--here`.
- The pet's stage is `desktop`, and where there is no desktop stage yet (GNOME, X11, macOS) the
  popup shows the error. Set `FAMILIARD_BROWSER_STAGE=terminal` in the browser's environment to use
  the terminal stage, which draws nowhere you can see and is mostly useful for tests.

## What it reads, and what it does not

**It reads:**

- Whether a stop button is on the page, by the selectors in `extension/src/detect.js`, and only in
  a conversation you brought a pet to.
- Whether the sidebar row of a conversation you brought a pet to says `working`, by the
  `data-sidebar-indicator` attribute, and whether that row is on screen. See
  `extension/src/sidebar.js`. It reads no title or text from the row.
- The conversation's id, from the URL.
- The title of the page, when you open the popup on a conversation, to suggest a name. It stays in
  the popup unless you press the button.

**It never reads** your messages, what you are typing, cookies, tokens or request bodies. It makes
no network requests. For a conversation you did not bring a pet to, it sends nothing at all and
does not look at the stop button or its row. The only thing it asks of the page as a whole is
whether any row carries that attribute, without looking at which.

**It sends**, only to the familiard on your own machine: the conversation's id, its state (`working`,
`done`, `ended`), and the pet and name you chose. Open pets hear those over loopback, like any
[hook](agents.md). familiard does not write them to disk, and they do not leave your machine.

It asks the browser for `nativeMessaging`, `storage` and access to `https://chatgpt.com/*`, and
nothing more. `extension/test/privacy.test.js` pins this against a fixture page. The host also
refuses any message with a field it does not expect, so nothing else can travel that way by accident.

## When ChatGPT changes

If the pet stops reacting while everything else looks right, the stop button probably changed. The
extension treats a selector that matches nothing as no signal, so it goes quiet rather than guessing.
If `data-sidebar-indicator` disappears, the extension falls back to the stop button alone, and pets
stop following conversations out of sight. Its values live in `extension/src/sidebar.js`.

1. In a conversation that is generating, open the developer tools and inspect the stop button.
2. Note its `data-testid`, or else its `aria-label`.
3. Add a line to `STOP_SELECTORS` in `extension/src/detect.js`. Selectors of the shapes
   `[attribute="value"]`, `tag[attribute="value"]` and `form tag[attribute="value"]` are what the
   tests understand.
4. Add a button to the first test in `extension/test/detect.test.js`, and run `node --test` in `extension/`.
5. Reload the extension.

A pet only knows about a conversation while its tab shows it. If you move to another conversation
while one is answering, or reload the tab, its pet stops working without celebrating, because that
answer can no longer be seen. Come back while it is still answering and the pet picks it up again.

chatgpt.com has been seen with no `data-testid` on this button, only an `aria-label` in the page's
language: `Detener` in Spanish, which is the one checked on the real site. `Stop` for English is
expected but unchecked, and other languages have no entry, so the pet stays quiet until yours is added.

## The pieces

| Part | What it does |
| --- | --- |
| `familiard browser install`, `uninstall` | Writes or removes the host manifests. It never touches a manifest that does not name familiard, and says so |
| `familiard native-host` | Length-prefixed JSON on stdio: `list`, `summon`, `signal`, `dismiss`. Refuses over 64 KiB, exits on EOF. The browser starts one per message |
| `familiard summon <id> --session <session>` | Binds a pet to that exact session, here `chatgpt:<conversation id>`. It works for any session, and leaves when that session reports `ended` |

A manifest cannot pass arguments, so its `path` is the binary itself, which recognises the arguments
a browser launches it with. It only does so for this extension's own ids:
`epmjbipjlabgmcignobpeeggbdjidllp` on Chromium, through the `key` in the manifest, and
`familiard@fullfran.github.io` on Firefox.

Manifests go here:

| Browsers | Linux | macOS | Windows, under `HKCU\SOFTWARE\` |
| --- | --- | --- | --- |
| Firefox, Zen | `~/.mozilla/native-messaging-hosts` | `~/Library/Application Support/Mozilla/NativeMessagingHosts` | `Mozilla\NativeMessagingHosts` |
| Chromium | `~/.config/chromium/NativeMessagingHosts` | `~/Library/Application Support/Chromium/NativeMessagingHosts` | `Chromium\NativeMessagingHosts` |
| Chrome | `~/.config/google-chrome/NativeMessagingHosts` | `~/Library/Application Support/Google/Chrome/NativeMessagingHosts` | `Google\Chrome\NativeMessagingHosts` |
| Brave | `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts` | `~/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts` | Same key as Chrome |

`XDG_CONFIG_HOME` replaces `~/.config`. On Windows the registry value points at a file in
familiard's own folder.

Install writes all of them, even for browsers you do not have, which leaves a small folder behind.
`familiard browser uninstall` removes the files, and each folder if nothing else is in it.

## What is known and what is not

| Fact | Status | Source |
| --- | --- | --- |
| Folders and keys for Firefox, Chrome and Chromium | Documented, except Chromium's Windows key, which comes from KeePassXC's source | [MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_manifests), [Chrome](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging) |
| Brave's folders, and its use of Chrome's key on Windows | From another project's source, not from Brave | [KeePassXC](https://github.com/keepassxreboot/keepassxc/blob/develop/src/browser/NativeMessageInstaller.cpp) |
| Zen and Firefox on Linux read `~/.mozilla`, not `~/.zen` | Tested on Zen 1.21.12b and Firefox 158: each loaded this extension from a temporary profile and reached the host. The full flow on a page was only run on Brave | [Zen #10622](https://github.com/zen-browser/desktop/issues/10622), [Gecko](https://searchfox.org/mozilla-central/source/toolkit/xre/nsXREDirProvider.cpp) |
| Zen on macOS reads `Mozilla/NativeMessagingHosts` | Reported by users. Not tested here | [Zen #13214](https://github.com/zen-browser/desktop/issues/13214) |
| Zen on Windows uses Mozilla's registry key | **Unknown**: assumed from Firefox | |
| Flatpak or Snap browsers | **Unknown**: their sandbox may hide the manifest or the binary | |
| One manifest for both families | Documented, from Chrome 121 and Firefox 121 | [MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background) |
| Unsigned add-ons on Firefox release, beta, Developer Edition, Nightly, ESR | Documented | [Extension Workshop](https://extensionworkshop.com/documentation/publish/signing-and-distribution-overview/) |
| A permanent unsigned add-on on Zen | **Unknown**, and reported not to work | [Zen #138](https://github.com/zen-browser/desktop/issues/138) |
| Windows pets surviving the host, via `CREATE_BREAKAWAY_FROM_JOB` | Documented for Firefox. **Not tested on Windows** | [MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging) |
| Zen 1.21.15b to 1.22b on macOS never starting a host through `connectNative` | Reported and fixed on Twilight. This extension uses `sendNativeMessage`; whether that was affected is **unknown** | [Zen #15123](https://github.com/zen-browser/desktop/issues/15123) |
| The real chatgpt.com, its current selectors, and its Spanish labels | **Unknown**: never visited | |
