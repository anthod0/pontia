<script lang="ts">
  import DesktopFrame from '../../src/shell/desktop/DesktopFrame.svelte'
  import BotChatPage from '../../src/pages/BotChatPage.svelte'

  let { botId }: { botId: string } = $props()
</script>

<DesktopFrame>
  {#snippet sidebar()}{/snippet}
  {#snippet topbar()}{/snippet}
  <BotChatPage {botId} />
</DesktopFrame>
