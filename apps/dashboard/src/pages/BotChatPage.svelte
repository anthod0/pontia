<script lang="ts">
  import { untrack } from 'svelte'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { createBotChat } from '../features/bots/botChat.svelte'
  import MobileBotChat from '../views/mobile/bots/MobileBotChat.svelte'
  import DesktopBotChat from '../views/desktop/bots/DesktopBotChat.svelte'
  let { botId }: { botId: string } = $props()
  const chat = createBotChat(untrack(() => botId))
</script>

{#if isMobileShell()}<MobileBotChat {chat} />{:else}<DesktopBotChat {chat} />{/if}
