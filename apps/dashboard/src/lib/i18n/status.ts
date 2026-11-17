import type { MessageKey, Translator } from "./index";

const labels: Record<string, MessageKey> = {
  created: "state.created",
  starting: "state.starting",
  idle: "state.idle",
  busy: "state.busy",
  waiting_for_interaction: "state.waiting_for_interaction",
  interrupted: "state.interrupted",
  exited: "state.exited",
  error: "state.error",
  queued: "state.queued",
  running: "state.running",
  completed: "state.completed",
  failed: "state.failed",
  abandoned: "state.abandoned",
  pending: "state.pending",
  paused: "state.paused",
  replanning: "state.replanning",
  blocked: "state.blocked",
  recovering: "state.recovering",
  exiting: "state.exiting",
  submitted: "state.submitted",
  unknown: "state.unknown",
  requested: "state.requested",
  preparing: "state.preparing",
  dispatching: "state.dispatching",
  open: "state.open",
  closed: "state.closed",
  connecting: "state.connecting",
  reconnecting: "state.reconnecting",
  applied: "state.applied",
  rejected: "state.rejected",
  resolved: "state.resolved",
  cancelled: "state.cancelled",
  accepted: "state.accepted",
};

export function statusLabel(state: string, t: Translator): string {
  return labels[state] ? t(labels[state]) : state;
}
