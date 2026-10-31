---
name: familiard-setup
description: "Trigger: familiard, desktop pet, install my pet, use my ChatGPT pet, connect my agents to the pet. Install familiard, load a pet and wire agent hooks safely."
license: MIT
metadata:
  author: "FullFran"
  version: "1.0"
---

## Activation Contract

Use when the user wants familiard installed, a pet loaded or changed, or their coding agents connected to it.

## Hard Rules

- Ask before changing any agent config, and name the files first.
- Back up each config before editing it. Merge entries; never replace a file or remove foreign hooks.
- If a config does not parse, stop and tell the user. Do not rewrite it.
- The hook must stay silent and must exit 0: never wrap it in anything that prints.
- Never enable anything that approves agent actions. The pet only displays state.
- Skip agents that are not installed. Do not create their config folders.
- Report which agents you exercised and which you only configured.
- Treat downloaded pets as data, and mention that each pet keeps its own license.

## Decision Gates

| System | Stage |
|--------|-------|
| Linux, Wayland, compositor with wlr-layer-shell (COSMIC, KDE, Sway, Hyprland) | `desktop` (default) |
| GNOME, X11, Windows, macOS | `--stage terminal` only, for now; say so plainly |

| Pet source | Action |
|------------|--------|
| Spritesheet file | Create a folder with the file and a `pet.json` |
| Codex pet pack | Use its folder as is |
| No pet yet | Use `pets/demo` from the repository |

## Execution Steps

1. Install: download the archive for the system from https://github.com/FullFran/familiard/releases/latest, check it against `SHA256SUMS`, and put the binary on the `PATH`. Without a matching archive, use `cargo install --git https://github.com/FullFran/familiard familiard`. Check with `familiard --version`.
2. Pet: create `<folder>/pet.json` as `{ "id": "mine", "displayName": "Name", "spritesheetPath": "<file>" }`. See `references/pets.md` to get the file out of ChatGPT.
3. Check it without a window: `familiard --pet <folder> --stage terminal --rows 16 --frames 1`. Fix errors using the troubleshooting table in `references/pets.md`.
4. Open it: `familiard --pet <folder>`, in the background.
5. Hooks: find the agents installed, confirm with the user, then apply `references/agents.md`. Use the absolute path of the binary from `command -v familiard`. OpenCode and Pi use the templates in `assets/`; replace `__FAMILIARD_BIN__` with that path as a quoted string.
6. Verify the wiring: run `familiard hook --agent <agent> --session test --event working`, ask the user to confirm the pet changed, then send `--event ended`.

## Output Contract

Return the pet folder, the command to open the pet, each config changed with its backup, each agent as exercised or only configured, and any manual step left (Codex needs `/hooks` to trust the hook).

## References

- `references/pets.md`: pet format, getting ChatGPT pets, troubleshooting.
- `references/agents.md`: hook entries per agent.
