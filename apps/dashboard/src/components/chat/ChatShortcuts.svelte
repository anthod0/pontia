<script lang="ts">
  import { useDeviceRuntime } from '../../device/context';
  import { navigate } from '$lib/navigation'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { recentActiveSessions, snapshotSessionOverview } from "../../queries/sessionOverview";
  import { adjacentSessionId, sessionIdAtShortcutIndex, sessionIdFromChatPath } from '$lib/shortcuts/sessionNavigation'
  import { useSessionPanel } from './sessionPanel/sessionPanelContext.svelte'
  import { registerShortcut } from '$lib/shortcuts/dispatch'
  const device = useDeviceRuntime();
  const { createSessionOverviewQuery } = device.sessionOverview;
  const { selectedWorkspaceId } = device.sidebarSessionFilter;

  let { newChatHandle }: { newChatHandle?: string } = $props()

  const sessionPanel = useSessionPanel()
  const overviewQuery = createSessionOverviewQuery()
  const overview = $derived(snapshotSessionOverview(overviewQuery.data))
  const shortcutSessions = $derived(recentActiveSessions(overview, $selectedWorkspaceId))

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

  registerShortcut('focusInput', focusChatInput)
  registerShortcut('toggleGit', () => { if (sessionPanel?.source) sessionPanel.toggleGit() })
  registerShortcut('newChat', openNewChatFromCurrentRoute)
  registerShortcut('openChatByPosition', event => {
    const sessionId = sessionIdAtShortcutIndex(shortcutSessions, event.code.slice(-1))
    if (sessionId) openChat(`/chat/${sessionId}`)
  })
  function openAdjacent(direction: 1 | -1) {
    const current = sessionIdFromChatPath(dashboardRelativePath())
    const sessionId = adjacentSessionId(shortcutSessions, current, direction)
    if (sessionId) openChat(`/chat/${sessionId}`)
  }
  registerShortcut('nextChat', () => openAdjacent(1))
  registerShortcut('previousChat', () => openAdjacent(-1))
</script>
