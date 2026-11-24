<script lang="ts">
  import { untrack } from 'svelte'
  import { page } from '$app/state'
  import { writable } from 'svelte/store'
  import MobileBotChat from '../../src/views/mobile/bots/MobileBotChat.svelte'
  import MobileOverlayHost from '../../src/shell/mobile/MobileOverlayHost.svelte'
  import { provideMobileTitle } from '../../src/shell/mobile/headerTitle'
  import { provideTopBarAction } from '../../src/components/layout/topBarAction'
  import { provideMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { createBotChat } from '../../src/features/bots/botChat.svelte'
  let { botId }: { botId: string } = $props()
  const title = provideMobileTitle(writable(null))
  const action = provideTopBarAction()
  const stack = provideMobileStack()
  const chat = createBotChat(untrack(() => botId))
  $effect(() => {
    const id = page.state.mobileLayer
    untrack(() => stack.reconcile(id))
  })
</script>

{@render $title?.()}{@render $action?.()}
<MobileBotChat {chat} />
<MobileOverlayHost {stack} />
