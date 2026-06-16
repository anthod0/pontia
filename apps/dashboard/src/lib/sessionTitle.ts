export function untitledSessionLabel(clientType: string | null | undefined): string {
  const normalized = clientType?.trim();
  return `Untitled ${normalized || 'agent'} session`;
}
