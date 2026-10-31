# Pets

A pet is a folder with a spritesheet and a `pet.json`. Add it to your gallery and bring it out:

```sh
familiard add ~/pets/mine
familiard summon mine
```

## Your gallery

| Command | What it does |
| --- | --- |
| `familiard add <folder> [--as id]` | Copies the pet in. The first one becomes the default |
| `familiard pets [--json]`, `-l` | Lists the gallery: `*` marks the default, `(open)` what is out |
| `familiard summon [id] [--here \| --session ID] [--name TEXT] [--at X,Y]`, `-s` | Brings out a pet, the default if none is named |
| `familiard dismiss [id]`, `--all`, `-d` | Sends pets away |
| `familiard default <id>` | Chooses the default |
| `familiard remove <id>` | Deletes a pet from the gallery, and sends it away if it is out |

Each open pet is its own small process, and each reacts to your agents as described in
[A pet for one session](#a-pet-for-one-session). The gallery lives
in `~/.local/share/familiard` (`%APPDATA%\\familiard` on Windows,
`~/Library/Application Support/familiard` on macOS), or wherever `FAMILIARD_HOME` points.

## A pet for one session

| You run | The pet |
| --- | --- |
| `familiard summon mochi` | Follows every session, never leaves by itself, and has no caption |
| `familiard summon mochi --here` | Follows only the next session in this folder, and leaves when it ends |
| `familiard summon mochi --name "Bug del login"` | Follows every session, with that caption |
| `familiard summon mochi --here --name "Bug del login"` | Both |
| `familiard summon mochi --session claude:abc` | Follows only that exact session, and leaves when it ends |

With `--here`, the pet stays idle until a session reports from this folder or one below it. That
session claims the pet, and from then on no other session counts. A session that goes quiet keeps
its pet; only its end sends the pet away, and `familiard dismiss` always works.

`--session` is for a session with no folder to match, such as [a ChatGPT conversation](browser.md).
It is the agent and the session's id as a hook reports them, `<agent>:<session>`, and it cannot be
combined with `--here`.

A pet can only be out once, so `summon` on a pet that is already out changes nothing about it.

The caption sits above the pet's head and says what the session is doing. Its text is the name
you gave, or else the last part of the folder:

| The session | The caption |
| --- | --- |
| Is idle, or has not started | `Bug del login` |
| Works | `Bug del login · working` |
| Needs you | `Bug del login · needs you` |
| Finished | `Bug del login · done` |

A long name is cut with an ellipsis so that the caption stays about 240 pixels wide. Clicks pass
through the caption on Wayland; on Windows it can be used to drag the pet. A terminal prints the
same text on its own line. Characters the font lacks show as a box.

## Use your ChatGPT pet

1. Get the spritesheet as a PNG or WebP file.
2. Put it in a new folder, next to this `pet.json`:

   ```json
   { "id": "mine", "displayName": "My pet", "spritesheetPath": "spritesheet.png" }
   ```

3. Run `familiard add <folder>`, then `familiard summon mine`.

### Getting the file

| Your pet | How |
| --- | --- |
| Uploaded on ChatGPT web | Settings > Personalization > Pet: uploaded pets have a download option |
| Created in a ChatGPT conversation | Ask ChatGPT for the spritesheet as a downloadable file |
| Created in the desktop app | Stored locally, but OpenAI does not document where, so look in the app or ask it to export the file |

Both sheet sizes work: 1536x1872 (version 1) and 1536x2288 (version 2, which adds rows of
looking poses that familiard does not use yet).

Any pack in the Codex pet format works the same way, including community ones. Each pet
keeps its own license, so check it before sharing one. A pack with no license, or a fan pack of
someone else's characters, is for your own gallery only.

## Spritesheet layout

Rows, top to bottom:

| Row | Animation | Plays when |
| --- | --- | --- |
| 0 | idle | Nothing is going on |
| 1, 2 | running right, running left | Not used yet |
| 3 | waving | An agent finished |
| 4 | jumping | Just after everything finished, for 3 seconds |
| 5 | failed | Not used yet |
| 6 | waiting | An agent needs you |
| 7 | running in place | An agent is working |
| 8 | review | Not used yet |

Frames are found through transparent gaps, not through a grid:

- Leave a fully transparent gap between rows and between frames, or they merge into one.
- Rows can have any number of frames. To hold a pose longer, repeat its frame.
- A pet needs at least the idle row; any other missing animation falls back to idle.

Speed is fixed per animation: idle and waving 3 fps, waiting 4, working and jumping 8.

## Troubleshooting

| Symptom | Cause |
| --- | --- |
| `a pet needs at least an idle animation` | The sheet has no visible pixels |
| One huge frame, or frames that flicker between two poses | No transparent gap between frames |
| The wrong animation plays | A row is empty, so the rows after it shift up |
| The background shows through the eyes or hair | The sheet has holes: background removal took those pixels too. Fix them in the sheet |

To check a pack without opening a window:

```sh
familiard run --pet <folder> --stage terminal --rows 16 --frames 1
```
