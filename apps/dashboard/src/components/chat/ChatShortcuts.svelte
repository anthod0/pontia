<script lang="ts">
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { navigate } from '$lib/navigation'
  import { sessions } from '../../stores/sessions'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as Kbd from '$lib/components/ui/kbd/index.js'
  import { adjacentActiveSessionId, isChatRoute, sessionIdAtShortcutIndex, sessionIdFromChatPath } from '$lib/shortcuts/sessionNavigation'

  type Shortcut = {
    keys: string[]
    label: string
    description: string
  }

  const shortcuts: Shortcut[] = [
    { keys: ['Alt', 'J'], label: 'Next active chat', description: 'Switch to the next active session chat.' },
    { keys: ['Alt', 'K'], label: 'Previous active chat', description: 'Switch to the previous active session chat.' },
    { keys: ['Alt', '1-9'], label: 'Open active chat by position', description: 'Open one of the first nine active session chats in sidebar order.' },
    { keys: ['Alt', 'N'], label: 'New chat', description: 'Open the new chat screen.' },
    { keys: ['Alt', 'L'], label: 'Focus chat input', description: 'Move focus to the new chat prompt or current session composer.' },
    { keys: ['Alt', '?'], label: 'Keyboard shortcuts', description: 'Show this shortcut reference.' },
  ]

  let shortcutsOpen = $state(false)

  onMount(() => {
    const openShortcuts = () => (shortcutsOpen = true)
    document.addEventListener('pontia:open-chat-shortcuts', openShortcuts)
    return () => document.removeEventListener('pontia:open-chat-shortcuts', openShortcuts)
  })

  function shouldIgnoreShortcut(event: KeyboardEvent): boolean {
    if (event.isComposing) return true
    const target = event.target instanceof Element ? event.target : document.activeElement
    if (!target) return false
    return Boolean(target.closest('input, textarea, select, [contenteditable="true"], [contenteditable=""]'))
  }

  function openChat(path: string, options?: Record<string, string>): void {
    if (options) navigate(path, options)
    else navigate(path)

  }

  function openNewChatFromCurrentRoute(): void {
    const currentSessionId = sessionIdFromChatPath(window.location.pathname)
    const currentSession = currentSessionId ? get(sessions).find((session) => session.session_id === currentSessionId) : null
    const workspaceId = currentSession?.workspace_id?.trim()
    if (workspaceId) openChat('/chat', { workspace: workspaceId })
    else openChat('/chat')
  }

  function focusChatInput(): void {
    const target = document.querySelector<HTMLElement>('[data-chat-shortcut-focus-target="true"]')
    target?.focus()
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (!event.altKey || event.ctrlKey || event.metaKey || !isChatRoute(window.location.pathname)) return

    const key = event.key.toLowerCase()
    if (key === '?' || (event.code === 'Slash' && event.shiftKey)) {
      event.preventDefault()
      shortcutsOpen = true
      return
    }

    if (key === 'l') {
      event.preventDefault()
      focusChatInput()
      return
    }

    if (shouldIgnoreShortcut(event)) return

    if (key === 'n') {
      event.preventDefault()
      openNewChatFromCurrentRoute()
      return
    }

    let sessionId: string | null = null
    if (key === 'j') {
      sessionId = adjacentActiveSessionId(get(sessions), sessionIdFromChatPath(window.location.pathname), 1)
    } else if (key === 'k') {
      sessionId = adjacentActiveSessionId(get(sessions), sessionIdFromChatPath(window.location.pathname), -1)
    } else if (/^[1-9]$/.test(key)) {
      sessionId = sessionIdAtShortcutIndex(get(sessions), key)
    }

    if (!sessionId) return
    event.preventDefault()
    openChat(`/chat/${sessionId}`)
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#snippet shortcutKeys(keys: string[])}
  <Kbd.Group class="shrink-0 text-muted-foreground">
    {#each keys as key}
      <Kbd.Root>{key}</Kbd.Root>
    {/each}
  </Kbd.Group>
{/snippet}

<Dialog.Root bind:open={shortcutsOpen}>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Keyboard shortcuts</Dialog.Title>
      <Dialog.Description>Chat shortcuts are available on New Chat and session chat pages.</Dialog.Description>
    </Dialog.Header>

    <div class="space-y-3 py-2">
      {#each shortcuts as shortcut}
        <div class="flex items-start justify-between gap-4 rounded-lg border bg-card/50 p-3">
          <div class="min-w-0 space-y-1">
            <div class="text-sm font-medium">{shortcut.label}</div>
            <div class="text-xs text-muted-foreground">{shortcut.description}</div>
          </div>
          {@render shortcutKeys(shortcut.keys)}
        </div>
      {/each}
    </div>
  </Dialog.Content>
</Dialog.Root>
