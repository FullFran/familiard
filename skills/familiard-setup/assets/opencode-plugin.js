// Reports OpenCode activity to familiard. Replace __FAMILIARD_BIN__ with the binary's
// path as a quoted string.
const FAMILIARD_BIN = __FAMILIARD_BIN__

export const Familiard = async ({ directory }) => {
  const sessions = new Set()
  const command = (state, session) => [FAMILIARD_BIN, "hook", "--agent", "opencode", "--event", state,
    "--session", session, ...(directory ? ["--project", directory] : [])]
  const quiet = { stdin: "ignore", stdout: "ignore", stderr: "ignore" }
  const send = (state, session) => {
    try {
      Bun.spawn(command(state, session), quiet)
    } catch {}
  }
  // OpenCode sends no event when it quits, so the last sessions are closed here.
  process.once("exit", () => {
    for (const session of sessions) {
      try {
        Bun.spawnSync(command("ended", session), quiet)
      } catch {}
    }
  })
  return {
    event: async ({ event }) => {
      const session = event?.properties?.sessionID || event?.properties?.info?.id
      if (!session) return
      if (event.type === "session.deleted") {
        sessions.delete(session)
        send("ended", session)
      } else if (event.type === "session.idle") {
        sessions.add(session)
        send("done", session)
      } else if (event.type === "session.status" && event.properties?.status?.type === "busy") {
        sessions.add(session)
        send("working", session)
      }
    },
  }
}
