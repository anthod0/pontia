<script lang="ts">
  import type { Bot } from '../../../api/types'
  import BotMessageSearch from '../../../components/bots/BotMessageSearch.svelte'
  let { bot, onselect }: { bot: Bot; onselect: (id: string) => void } = $props()
</script>

<div class="mobile-bot-screen flex h-full flex-col"><BotMessageSearch {bot} active selected={null} {onselect} /></div>
