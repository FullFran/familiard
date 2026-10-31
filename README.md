# familiard

A very light desktop pet that lives on your screen. It can be dragged between monitors,
lets clicks through its transparent areas, and reacts to your coding agents: working,
waiting for you, celebrating when they finish. Early stage.

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

## Reacting to agents

Point your agent's hooks at `familiard hook --agent <name>`, and it tells the running pet
over a loopback socket. In Claude Code's `settings.json`:

```json
{ "hooks": { "Stop": [ { "hooks": [
  { "type": "command", "command": "familiard hook --agent claude" }
] } ] } }
```

Use it for `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `Notification`, `Stop` and
`SessionEnd`. It knows the events of `claude`, `codex`, `gemini`, `antigravity` and
`opencode`. Anything else can report directly, including your own scripts:

```sh
familiard hook --agent build --session ci --event working   # registered, working, waiting, done, ended
```

The hook prints nothing and always exits 0, so it never gets in an agent's way. If the pet
is closed the event is dropped. Only local programs can reach the pet, and all they can do
is change its mood. Set `FAMILIARD_PORT` to use another port than 47629.

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
 ├─ agents           ActivityFeed: hears hooks over a loopback socket
 └─ application      ports (PetSource, Stage, ActivityFeed) and the Player use case
     └─ domain       Pet, Companion, Activity (sessions to mood), Sprite
```

To add a display (Windows, macOS, an editor panel), implement `Stage`. A stage owns its
event loop and asks the `Player` what to draw. To add a pet format, implement `PetSource`.
To hear about activity some other way, implement `ActivityFeed`.

## License

MIT. Pets you load keep their own licenses. The agent event tables are ported from
[AgentPet](https://github.com/ntd4996/agentpet) (MIT).
