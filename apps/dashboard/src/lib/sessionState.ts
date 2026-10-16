import type { SessionView } from "../api/types";

export function isTerminalSession(session: Pick<SessionView, "state">): boolean {
  return session.state === "exited" || session.state === "error";
}

export function sessionStateDotClass(state: string): string {
  switch (state) {
    case "busy":
    case "starting":
      return "bg-busy";
    case "idle":
      return "bg-idle";
    case "interrupted":
      return "bg-interrupted";
    case "error":
      return "bg-error";
    default:
      return "bg-inactive";
  }
}
