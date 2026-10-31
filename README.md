# familiard

A very light desktop pet that lives on your screen. Early stage: it plays its idle
animation and can be dragged around; it does not react to coding agents yet.

```sh
cargo run --release -- --pet pets/demo                     # floating on the desktop
cargo run --release -- --pet pets/demo --stage terminal    # inside a truecolor terminal
```

Options: `--at X,Y` start position, `--rows N` terminal height, `--frames N` stop after
N frames in the terminal.

## Where it runs

| Stage | Platforms | State |
| --- | --- | --- |
| `desktop` | Linux on Wayland with wlr-layer-shell: COSMIC, KDE, Sway, Hyprland | Works on COSMIC, the rest untested |
| `desktop` | GNOME, X11, Windows, macOS | Not written yet |
| `terminal` | Any truecolor terminal | Works on Linux; built for Windows and macOS by CI |

On COSMIC, a pet of about 80 frames at 120x180 measured 6 MB of memory (PSS) and 0.1% CPU while idle.
It reaches about 50 MB for a moment while it decodes the sheet at start.

## Pets

A pet is a folder with a `pet.json` and a spritesheet, in the Codex pet layout:

```json
{ "id": "demo", "displayName": "Demo", "spritesheetPath": "spritesheet.png" }
```

Sheet rows, top to bottom: idle, running right, running left, waving, jumping, failed,
waiting, working, review. Frames are found through the transparent gaps between them, so
rows can have any number of frames and no fixed grid is needed. `pets/demo` is
drawn by `pets/demo/generate.py`.

## Architecture

Hexagonal. Dependencies point inward, and the domain has none at all.

```
cli (composition root)
 ├─ pack             PetSource: reads pet folders
 ├─ stage-wayland    Stage: floating surface on Wayland
 ├─ stage-terminal   Stage: half blocks in a terminal
 └─ application      ports (PetSource, Stage) and the Player use case
     └─ domain       Pet, Mood, Companion, Sprite
```

To add a display (Windows, macOS, an editor panel), implement `Stage`. A stage owns its
event loop and asks the `Player` what to draw. To add a pet format, implement `PetSource`.

## License

MIT. Pets you load keep their own licenses.
