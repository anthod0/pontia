export function settingsRedirectPath(handle?: string): string {
  return `/${handle ?? ""}/settings/common`;
}
