import type { SessionView } from "../api/types";

export function isExecutingSessionState(state: string | undefined): boolean {
  return state === "busy" || state === "waiting_for_interaction";
}

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
    case "waiting_for_interaction":
      return "bg-interrupted";
    case "interrupted":
      return "bg-interrupted";
    case "error":
      return "bg-error";
    default:
      return "bg-inactive";
  }
}
