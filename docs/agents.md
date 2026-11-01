# Reacting to coding agents

Agents run a command on each event (a hook). That command is `familiard hook`, which tells
every open pet over a loopback socket:

| What the agent does | The pet |
| --- | --- |
| Starts a session | Stays idle |
| Works | Works |
| Needs you | Waits |
| Finishes | Jumps for 3 seconds, waves for 30, then back to idle |
| Goes quiet for 5 minutes | Forgets that session |

With several sessions, working wins over waiting, and waiting over finished.

A pet summoned with `--here` hears one session instead of all of them: the next one to report
from that folder or below it. See [docs/pets.md](pets.md#a-pet-for-one-session). It leaves when
that session reports `ended`, so it needs an agent that reports it: Claude Code, Gemini CLI, Pi
and OpenCode do. With Codex and Antigravity, send it away with `familiard dismiss`.

A pet summoned with `--pane` works the same way, with the next session to report from that terminal
pane. The hook reads the pane from `TMUX_PANE`, `ZELLIJ_PANE_ID` (with `ZELLIJ_SESSION_NAME`),
`WEZTERM_PANE` or `KITTY_WINDOW_ID`, so it needs an agent that passes its environment to its hooks.
Only Claude Code has been checked; open pets hear the pane's name over loopback, like the folder.

`familiard` must be on the `PATH` of the agent, or use its full path. Merge these entries
into your config rather than replacing the file.

Tested end to end: Claude Code. The others follow their documented hook formats but have
not all been exercised yet, so please report what you find.

## Claude Code

`~/.claude/settings.json`, repeating the entry for `SessionStart`, `UserPromptSubmit`,
`PreToolUse`, `Notification`, `Stop` and `SessionEnd`:

```json
{ "hooks": { "Stop": [ { "hooks": [
  { "type": "command", "command": "familiard hook --agent claude" }
] } ] } }
```

## Codex

`~/.codex/hooks.json`, same shape with `--agent codex`, for `SessionStart`,
`UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `Stop` and `SubagentStop`. Then
enable hooks in `~/.codex/config.toml` and trust the hook by running `/hooks` in Codex:

```toml
[features]
hooks = true
```

## Gemini CLI

`~/.gemini/settings.json`, same shape with `--agent gemini`, for `SessionStart`,
`BeforeAgent`, `BeforeTool`, `AfterTool`, `Notification`, `AfterAgent` and `SessionEnd`.

## Antigravity

`~/.gemini/config/hooks.json`. Tool events take a matcher and the others do not:

```json
{ "familiard": {
  "PreInvocation": [ { "type": "command", "command": "familiard hook --agent antigravity" } ],
  "Stop": [ { "type": "command", "command": "familiard hook --agent antigravity" } ],
  "PreToolUse": [ { "matcher": "*", "hooks": [ { "type": "command", "command": "familiard hook --agent antigravity" } ] } ],
  "PostToolUse": [ { "matcher": "*", "hooks": [ { "type": "command", "command": "familiard hook --agent antigravity" } ] } ]
} }
```

## OpenCode and Pi

They load code rather than commands. Copy the template from
`skills/familiard-setup/assets` in the repository:

| Agent | File | Goes to |
| --- | --- | --- |
| OpenCode | `opencode-plugin.js` | `~/.config/opencode/plugins/familiard.js` |
| Pi | `pi-extension.ts` | `~/.pi/agent/extensions/familiard.ts` |

## Anything else

Any script can report. Signals are `registered`, `working`, `waiting`, `done` and `ended`:

```sh
familiard hook --agent build --session nightly --event working
familiard hook --agent build --session nightly --event done
```

The folder a session runs in is what `--here` matches. It comes from the agent's JSON (`cwd`,
`projectRoot`, or the first of `workspace_roots` or `workspacePaths`), else from
`--project <dir>`, else from the directory the hook runs in. Symlinks are resolved when the folder
exists.

## What the hook guarantees

- It prints nothing and always exits 0, so it cannot get in an agent's way.
- If no pet is open, the event is dropped and the hook returns at once.
- The pet listens on loopback only. Local programs can change its mood or send it away, and nothing else.
- The pet never answers an agent or approves anything.
