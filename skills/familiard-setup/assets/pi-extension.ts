// Reports Pi activity to familiard. Replace __FAMILIARD_BIN__ with the binary's path as a
// quoted string.
import { spawn } from "node:child_process"

const FAMILIARD_BIN = __FAMILIARD_BIN__

export default function (pi: any) {
  const send = (state: string, ctx: any) => {
    try {
      const session = ctx?.sessionManager?.getSessionFile?.() || ctx?.cwd || process.cwd()
      const child = spawn(FAMILIARD_BIN, ["hook", "--agent", "pi", "--event", state, "--session", session, "--project", ctx?.cwd || process.cwd()], {
        stdio: "ignore",
        detached: true,
      })
      child.on("error", () => {})
      child.unref()
    } catch {}
  }
  pi.on("session_start", async (_event: any, ctx: any) => send("registered", ctx))
  pi.on("agent_start", async (_event: any, ctx: any) => send("working", ctx))
  pi.on("agent_end", async (_event: any, ctx: any) => send("done", ctx))
  pi.on("session_shutdown", async (_event: any, ctx: any) => send("ended", ctx))
}
