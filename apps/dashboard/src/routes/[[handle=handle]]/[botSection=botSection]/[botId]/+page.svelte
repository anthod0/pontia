<script lang="ts">
  import { page } from '$app/state'
  import { rememberLastPage } from '$lib/lastPage'
  import { useDeviceRuntime } from '../../../../device/context'
  import BotChatPage from '../../../../pages/BotChatPage.svelte'

  const device = useDeviceRuntime()
  $effect(() => rememberLastPage(device.id, { kind: 'bot', id: page.params.botId! }))
</script>

{#key page.params.botId}
  <BotChatPage botId={page.params.botId!} />
{/key}
