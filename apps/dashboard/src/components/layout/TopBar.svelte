<script lang="ts">
  import { useDeviceRuntime } from '../../device/context';
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'
  import { snapshotSessionOverview } from "../../queries/sessionOverview";
  const device = useDeviceRuntime();
  const { sessionDetail, sessionDetailError, updateSessionTitle } = device.sessions;
  const { createSessionOverviewQuery } = device.sessionOverview;

  let currentPath = $state(dashboardRelativePath())
  const overviewQuery = createSessionOverviewQuery()
  const overview = $derived(snapshotSessionOverview(overviewQuery.data))
  let renameDialogOpen = $state(false)
  let renameBusy = $state(false)
  let renameError = $state<string | null>(null)

  function updatePath(): void {
    currentPath = dashboardRelativePath()
  }

  function updatePathAfterNavigation(): void {
    setTimeout(updatePath, 0)
  }

  function isChatPath(path: string): boolean {
    return path === '/' || path.startsWith('/chat/')
  }

  const sessionId = $derived(currentPath.startsWith('/chat/') ? decodeURIComponent(currentPath.split('/')[2] ?? '') : '')
  const overviewSession = $derived(
    overview.pinned.find((item) => item.session_id === sessionId)
      ?? overview.active.find((item) => item.session_id === sessionId)
      ?? overview.list.find((item) => item.session_id === sessionId)
      ?? null
  )
  const session = $derived(sessionId
    ? ($sessionDetail?.session.session_id === sessionId ? $sessionDetail.session : ($sessionDetailError ? null : overviewSession))
    : null)
  const title = $derived(session ? sessionChatTitle(session) : '')
  const chatPath = $derived(isChatPath(currentPath))
  const groupClass = $derived(['flex min-w-0 items-center gap-1', chatPath && 'pointer-events-auto h-8 rounded-lg bg-popover px-1 ring-1 ring-foreground/10', chatPath && !title && 'md:hidden'])

  function openRenameDialog(): void {
    renameError = null
    renameDialogOpen = true
  }

  async function confirmRename(nextTitle: string | null): Promise<void> {
    if (!session) return
    renameBusy = true
    renameError = null
    try {
      await updateSessionTitle(session.session_id, nextTitle)
      renameDialogOpen = false
    } catch (error) {
      renameError = error instanceof Error ? error.message : String(error)
    } finally {
      renameBusy = false
    }
  }
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<header class={['sticky z-10 flex items-center px-3 md:px-4', chatPath ? 'pointer-events-none h-12' : 'h-8 bg-surface']} style="top: var(--visual-viewport-top, 0px)">
  <div class={groupClass}>
    <Sidebar.Trigger class="md:hidden" />
    {#if title}
      <h1 class="mx-1 hidden min-w-0 max-w-80 md:block">
        <button
          type="button"
          class="group flex w-full min-w-0 items-center gap-1.5 rounded-md px-2 py-0.5 text-left text-sm font-normal text-heading focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          title={`Rename ${title}`}
          onclick={openRenameDialog}
        >
          <span class="min-w-0 truncate">{title}</span>
          <PencilSimpleIcon aria-hidden="true" class="hidden size-3.5 shrink-0 text-muted-foreground group-hover:block group-focus-visible:block" />
        </button>
      </h1>
    {/if}
  </div>
</header>

<RenameSessionDialog
  bind:open={renameDialogOpen}
  {session}
  busy={renameBusy}
  error={renameError}
  onConfirm={(nextTitle) => void confirmRename(nextTitle)}
/>
