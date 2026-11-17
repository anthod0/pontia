<script lang="ts">
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import { onMount } from 'svelte'
  import { navigate } from '$lib/navigation'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { recentActiveSessions, snapshotSessionOverview } from "../../queries/sessionOverview";
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import ShortcutKeys from '$lib/components/shortcuts/ShortcutKeys.svelte'
  import { adjacentSessionId, isChatRoute, sessionIdAtShortcutIndex, sessionIdFromChatPath } from '$lib/shortcuts/sessionNavigation'
  import { helpShortcuts, messageInputShortcuts, matchChatShortcut } from '$lib/shortcuts/definitions'
  const device = useDeviceRuntime();
  const { createSessionOverviewQuery } = device.sessionOverview;
  const { selectedWorkspaceId } = device.sidebarWorkspaceFilter;

  let { newChatHandle }: { newChatHandle?: string } = $props()

  let shortcutsOpen = $state(false)
  const overviewQuery = createSessionOverviewQuery()
  const overview = $derived(snapshotSessionOverview(overviewQuery.data))
  const shortcutSessions = $derived(recentActiveSessions(overview, $selectedWorkspaceId))

  onMount(() => {
    const openShortcuts = () => (shortcutsOpen = true)
    document.addEventListener('pontia:open-chat-shortcuts', openShortcuts)
    return () => document.removeEventListener('pontia:open-chat-shortcuts', openShortcuts)
  })

  function openChat(path: string, options?: Record<string, string>): void {
    if (newChatHandle) void navigate(path, options, { scope: `/${newChatHandle}` })
    else if (options) void navigate(path, options)
    else void navigate(path)

  }

  function openNewChatFromCurrentRoute(): void {
    const currentSessionId = sessionIdFromChatPath(dashboardRelativePath())
    const currentSession = currentSessionId
      ? [overview.pinned, overview.active, overview.list]
          .flat()
          .find((session) => session.session_id === currentSessionId)
      : null
    const workspaceId = currentSession?.workspace_id?.trim()
    if (workspaceId) openChat('/', { workspace: workspaceId })
    else openChat('/')
  }

  function focusChatInput(): void {
    const target = document.querySelector<HTMLElement>('[data-chat-shortcut-focus-target="true"]')
    target?.focus()
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (!isChatRoute(dashboardRelativePath())) return
    const shortcut = matchChatShortcut(event)
    if (!shortcut) return

    if (shortcut.action === 'showHelp') {
      event.preventDefault()
      shortcutsOpen = true
      return
    }
    if (shortcut.action === 'focusInput') {
      event.preventDefault()
      focusChatInput()
      return
    }
    // New Chat selectors switch themselves; consume the key here so Option+letter does not type a symbol.
    if (shortcut.action === 'switchDevice' || shortcut.action === 'switchAgentClient' || shortcut.action === 'switchWorkspace') {
      event.preventDefault()
      return
    }
    if (shortcut.action === 'newChat') {
      event.preventDefault()
      openNewChatFromCurrentRoute()
      return
    }

    const currentSessionId = sessionIdFromChatPath(dashboardRelativePath())
    const sessionId = shortcut.action === 'openChatByPosition'
      ? sessionIdAtShortcutIndex(shortcutSessions, shortcut.position)
      : adjacentSessionId(shortcutSessions, currentSessionId, shortcut.action === 'nextChat' ? 1 : -1)
    // Consume the key even when there is nowhere to go, so Option+letter does not type a symbol.
    event.preventDefault()
    if (sessionId) openChat(`/chat/${sessionId}`)
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<Dialog.Root bind:open={shortcutsOpen}>
  <Dialog.Content class="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{$t('shortcuts.title')}</Dialog.Title>
      <Dialog.Description>{$t('shortcuts.description')}</Dialog.Description>
    </Dialog.Header>

    <div class="space-y-1">
      {#each helpShortcuts as shortcut}
        <div class="flex items-center justify-between gap-4 py-1.5">
          <div class="min-w-0 text-sm">{$t(shortcut.label)}</div>
          <ShortcutKeys keys={shortcut.keys} class="inline-flex shrink-0 border border-border bg-transparent" />
        </div>
      {/each}
    </div>

    <section class="space-y-1 border-t pt-4" aria-labelledby="message-input-shortcuts-title">
      <h3 id="message-input-shortcuts-title" class="font-medium">{$t('shortcuts.messageInput')}</h3>
      {#each messageInputShortcuts as shortcut}
        <div class="flex items-center justify-between gap-4 py-1.5">
          <div class="min-w-0 text-sm">{$t(shortcut.label)}</div>
          <div class="flex flex-wrap justify-end gap-2">
            {#each shortcut.keyGroups as keys}
              <ShortcutKeys {keys} class="inline-flex shrink-0 border border-border bg-transparent" />
            {/each}
          </div>
        </div>
      {/each}
      <p class="text-xs text-muted-foreground">{$t('shortcuts.inputSuggestions')}</p>
    </section>
  </Dialog.Content>
</Dialog.Root>
