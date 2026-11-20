<script lang="ts">
  import AppShell from '../../src/components/layout/AppShell.svelte'
  import BotChatPage from '../../src/pages/BotChatPage.svelte'

  let { botId }: { botId: string } = $props()
</script>

<AppShell>
  {#snippet sidebar()}{/snippet}
  {#snippet topbar()}{/snippet}
  <BotChatPage {botId} />
</AppShell>
