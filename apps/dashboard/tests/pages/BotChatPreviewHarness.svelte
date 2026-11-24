<script lang="ts">
  import DesktopFrame from '../../src/shell/desktop/DesktopFrame.svelte'
  import BotChatPage from '../../src/pages/BotChatPage.svelte'

  import {provideSessionPanel} from '../../src/components/chat/sessionPanel/sessionPanelContext.svelte'
  provideSessionPanel()
  let { botId }: { botId: string } = $props()
</script>

<DesktopFrame>
  {#snippet sidebar()}{/snippet}
  {#snippet topbar()}{/snippet}
  <BotChatPage {botId} />
</DesktopFrame>
