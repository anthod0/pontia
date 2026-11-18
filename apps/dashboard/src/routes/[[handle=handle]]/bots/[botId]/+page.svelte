<script lang="ts">
  import { page } from '$app/state'
  import BotChatPage from '../../../../pages/BotChatPage.svelte'
</script>

{#key page.params.botId}
  <BotChatPage botId={page.params.botId!} />
{/key}
