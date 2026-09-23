import type { SessionView } from '../api/types';

export function modelPickerDisabledReason(session: SessionView): string | undefined {
  if (session.capabilities.list_models !== true) return 'This client does not support model listing.';
  if (session.state !== 'idle' && session.state !== 'busy') return 'The session must be running to choose a model.';
  return session.model_control_unavailable_reason ?? undefined;
}
