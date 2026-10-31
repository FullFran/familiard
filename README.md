# familiard

A very light desktop pet that lives on your screen. It can be dragged between monitors,
lets clicks through its transparent areas, and reacts to your coding agents: working,
waiting for you, celebrating when they finish. Early stage.

```sh
cargo install --git https://github.com/FullFran/familiard familiard
git clone https://github.com/FullFran/familiard && cd familiard

familiard --pet pets/demo                     # floating on the desktop
familiard --pet pets/demo --stage terminal    # inside a truecolor terminal
```

**Let your AI agent do it.** Tell Claude Code, Codex or any agent that reads skills:

> Follow https://github.com/FullFran/familiard/blob/main/skills/familiard-setup/SKILL.md to
> install familiard, use my pet and connect my agents.

To install the skill for good, copy `skills/familiard-setup` to `~/.claude/skills/`.
It asks before touching any config and backs each one up first.

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

## Use your ChatGPT pet

Put the spritesheet in a folder, next to a `pet.json`, and point familiard at it:

```json
{ "id": "mine", "displayName": "My pet", "spritesheetPath": "spritesheet.png" }
```

```sh
familiard --pet path/to/folder
```

Any pet in the Codex format works. [docs/pets.md](docs/pets.md) explains how to get the file
out of ChatGPT and how to draw your own.

## React to your agents

Point your agent's hooks at `familiard hook --agent <name>`:

```json
{ "hooks": { "Stop": [ { "hooks": [
  { "type": "command", "command": "familiard hook --agent claude" }
] } ] } }
```

The hook is silent, always exits 0, and only reaches this machine. Every agent's setup is in
[docs/agents.md](docs/agents.md).

## Architecture

Hexagonal: the domain depends on nothing, and displays, pet formats and activity sources are
adapters. A new place to show the pet is one crate. See [docs/architecture.md](docs/architecture.md).

## License

MIT. Pets you load keep their own licenses. The agent event tables are ported from
[AgentPet](https://github.com/ntd4996/agentpet) (MIT).
