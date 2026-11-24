<script lang="ts">
  import { provideMobileHeader } from '../../src/shell/mobile/header'
  import { untrack } from 'svelte'
  import { page } from '$app/state'
  import MobileBotChat from '../../src/views/mobile/bots/MobileBotChat.svelte'
  import MobileOverlayHost from '../../src/shell/mobile/MobileOverlayHost.svelte'
  import { provideMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { createBotChat } from '../../src/features/bots/botChat.svelte'
  let { botId }: { botId: string } = $props()
  const { title, action } = provideMobileHeader(() => {})
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
