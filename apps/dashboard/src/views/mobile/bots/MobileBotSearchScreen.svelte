<script lang="ts">
  import type { Bot } from '../../../api/types'
  import BotMessageSearch from '../../../components/bots/BotMessageSearch.svelte'
  let { bot, fileSessionId, onselect }: {
    bot: Bot
    fileSessionId: string | null
    onselect: (id: string, path: string | null) => void
  } = $props()
</script>

<div class="mobile-bot-screen flex h-full flex-col"><BotMessageSearch {bot} active {fileSessionId} selected={null} {onselect} /></div>
