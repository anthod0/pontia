import type { EventView, JsonObject, SessionView } from '../api/types'

export interface ApprovalRequestView {
  requestEventId: string
  toolName: string
  permissionSuggestions: JsonObject[]
}

function record(value: unknown): JsonObject | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as JsonObject
    : null
}

export function approvalRequestFromSnapshot(
  session: SessionView | null | undefined,
  events: EventView[],
): ApprovalRequestView | null {
  const interaction = record(session?.metadata.interaction)
  if (
    interaction?.type !== 'approval'
    || interaction.state !== 'awaiting'
    || typeof interaction.request_event_id !== 'string'
  ) {
    return null
  }
  const request = events.find((event) =>
    event.event_id === interaction.request_event_id
    && event.type === 'approval.requested'
  )
  if (!request || typeof request.payload.tool_name !== 'string') return null
  const permissionSuggestions = Array.isArray(request.payload.permission_suggestions)
    ? request.payload.permission_suggestions
        .map(record)
        .filter((suggestion): suggestion is JsonObject => suggestion !== null)
    : []
  return {
    requestEventId: request.event_id,
    toolName: request.payload.tool_name,
    permissionSuggestions,
  }
}
