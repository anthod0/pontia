export function promptValueAfterEnter(value: string): string | null {
  const trimmedEnd = value.trimEnd()
  if (!trimmedEnd.endsWith('\\')) return null
  return `${trimmedEnd.slice(0, -1)}\n`
}
