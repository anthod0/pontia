<script lang="ts">
  import { onMount, type Snippet } from 'svelte'
  import { dashboardRelativePath, navigate } from '$lib/navigation'
  import { endReturnToLastPage, lastPagePath, takeLastPage } from '$lib/lastPage'
  import SessionConversationSkeleton from '$lib/components/session-chat/SessionConversationSkeleton.svelte'
  import BotChatSkeleton from '../components/bots/BotChatSkeleton.svelte'
  import MobileBotChatSkeleton from '../views/mobile/bots/MobileBotChatSkeleton.svelte'
  import { useDeviceRuntime } from '../device/context'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import NewChatPage from './NewChatPage.svelte'

  // A newly opened Dashboard and "Back to chat" reopen the last Session chat or Bot chat; otherwise this is New Chat.
  let { routeScope, deviceSelector }: { routeScope?: string; deviceSelector?: Snippet<[boolean]> } = $props()
  const device = useDeviceRuntime()
  let restoring = $state(takeLastPage(device.id))

  $effect(() => {
    if (!restoring && isMobileShell()) void navigate('/bots', undefined, { scope: routeScope, replaceState: true })
  })

  onMount(() => {
    if (!restoring) return
    let disposed = false
    const page = restoring
    if (page.kind === 'bot') {
      // The Bot chat opens at once, before the device answers; a Bot deleted since then is forgotten afterwards.
      const opened = `/bots/${encodeURIComponent(page.id)}`
      endReturnToLastPage()
      void navigate(opened, undefined, { scope: routeScope, replaceState: true })
      void lastPagePath(device, page).then(path => {
        if (!path && dashboardRelativePath() === opened) void navigate('/', undefined, { scope: routeScope, replaceState: true })
      })
      return
    }
    void lastPagePath(device, page).then(path => {
      if (disposed) return
      endReturnToLastPage()
      if (path) void navigate(path, undefined, { scope: routeScope, replaceState: true })
      else restoring = null
    })
    return () => { disposed = true }
  })
</script>

{#if restoring?.kind === 'chat'}
  <div class="mx-auto w-full max-w-[760px]"><SessionConversationSkeleton class="min-h-80" /></div>
{:else if restoring?.kind === 'bot'}
  {#if isMobileShell()}<MobileBotChatSkeleton />{:else}<BotChatSkeleton />{/if}
{:else if !isMobileShell()}
  <NewChatPage {routeScope} {deviceSelector} />
{/if}
