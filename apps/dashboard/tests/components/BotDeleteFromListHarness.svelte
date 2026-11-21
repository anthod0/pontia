<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { testDevice } from '../deviceFixture'
  import BotDeleteDialog from '../../src/components/bots/BotDeleteDialog.svelte'

  let { botId, ondone }: { botId: string; ondone: () => void } = $props()
  // Like the top bar, render the dialog only while the Bot is still listed.
  const bots = createQuery(
    () => ({ queryKey: ['bots'], queryFn: ({ signal }) => testDevice.api.listBots(signal) }),
    () => testDevice.queryClient,
  )
  const bot = $derived(bots.data?.find((item) => item.bot_id === botId))
</script>

{#if bot}
  <BotDeleteDialog {bot} onclose={() => {}} {ondone} />
{/if}
