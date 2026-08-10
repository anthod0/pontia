const focusedEntriesByWindow = new WeakMap<Window, Set<string>>()

export function claimChatEntryAutofocus(entryKey: string): boolean {
  if (typeof window === 'undefined') return false
  let focusedEntries = focusedEntriesByWindow.get(window)
  if (!focusedEntries) {
    focusedEntries = new Set<string>()
    focusedEntriesByWindow.set(window, focusedEntries)
  }
  if (focusedEntries.has(entryKey)) return false
  focusedEntries.add(entryKey)
  return true
}
