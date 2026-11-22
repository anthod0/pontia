<script lang="ts">
  import { onMount, type Snippet } from 'svelte'
  import { navigate } from '$lib/navigation'
  import { endReturnToLastPage, lastPagePath, takeLastPage } from '$lib/lastPage'
  import SessionConversationSkeleton from '$lib/components/session-chat/SessionConversationSkeleton.svelte'
  import BotChatSkeleton from '../components/bots/BotChatSkeleton.svelte'
  import { useDeviceRuntime } from '../device/context'
  import NewChatPage from './NewChatPage.svelte'

  // A newly opened Dashboard and "Back to chat" reopen the last Session chat or Bot chat; otherwise this is New Chat.
  let { routeScope, deviceSelector }: { routeScope?: string; deviceSelector?: Snippet<[boolean]> } = $props()
  const device = useDeviceRuntime()
  let restoring = $state(takeLastPage(device.id))

  onMount(() => {
    if (!restoring) return
    let disposed = false
    void lastPagePath(device, restoring).then(path => {
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
  <BotChatSkeleton />
{:else}
  <NewChatPage {routeScope} {deviceSelector} />
{/if}
