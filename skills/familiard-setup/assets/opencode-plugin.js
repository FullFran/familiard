// Reports OpenCode activity to familiard. Replace __FAMILIARD_BIN__ with the binary's
// path as a quoted string.
const FAMILIARD_BIN = __FAMILIARD_BIN__

export const Familiard = async ({ directory }) => {
  const send = (state) => {
    try {
      Bun.spawn([FAMILIARD_BIN, "hook", "--agent", "opencode", "--event", state,
        "--session", directory || "default"], { stdin: "ignore", stdout: "ignore", stderr: "ignore" })
    } catch {}
  }
  return {
    event: async ({ event }) => {
      if (event?.type === "session.idle") send("done")
      else if (event?.type === "session.status" && event.properties?.status?.type === "busy") send("working")
    },
  }
}
