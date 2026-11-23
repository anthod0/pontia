import type { MessageKey } from "$lib/i18n";
import type { MemoryKind } from "../../api/types";

const labels: Record<MemoryKind, MessageKey> = {
  fact: "memory.kind.fact",
  decision: "memory.kind.decision",
  event: "memory.kind.event",
  lesson: "memory.kind.lesson",
  observation: "memory.kind.observation",
};

export function memoryKindLabel(kind: MemoryKind): MessageKey {
  return labels[kind];
}
