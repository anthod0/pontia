<script lang="ts">
  import BotAvatar from '../../components/bots/BotAvatar.svelte'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { useDeviceRuntime } from '../../device/context'
  import { useMobileDirectory } from '../../features/bots/mobileDirectory'
  import { createBotTodos } from '../../features/bots/botTodos.svelte'
  import type { BotMessage } from '../../api/types'
  const directory = useMobileDirectory()!
  const todos = createBotTodos(useDeviceRuntime(), directory)
  const resolved = $derived(todos.groups.flatMap(({bot,result}) =>
    (result.data?.recently_resolved ?? []).map(item => ({bot,item}))))
</script>

{#snippet card(botId: string, item: BotMessage)}
  <button class="block w-full rounded-xl border bg-card p-3 text-start" onclick={() => navigate('/bots/' + encodeURIComponent(botId))}>
    <MessageTime value={item.created_at} class="text-muted-foreground" />
    <span class="line-clamp-3 whitespace-pre-wrap break-words">{item.content}</span>
  </button>
{/snippet}
<div class="grid gap-5">
  {#each todos.groups as {bot,result} (bot.bot_id)}
    <section class="grid gap-2"><h2><button class="flex min-h-11 items-center gap-2 font-semibold" onclick={() => navigate('/bots/' + encodeURIComponent(bot.bot_id))}>
        <BotAvatar class="size-7" shape={bot.avatar_shape} color={bot.avatar_color} />{bot.name}
      </button></h2>
      {#if result.error}<p role="alert">{result.error.message}</p>{:else if result.isPending}<p role="status">{$t('common.loading')}</p>{/if}
      {#each result.data?.open ?? [] as item (item.id)}{@render card(bot.bot_id,item)}{/each}
    </section>
  {:else}
    <p role="status" class="py-6 text-muted-foreground">{directory.error?.message ?? $t(directory.pending ? 'common.loading' : 'bots.noTodos')}</p>
  {/each}
  {#if resolved.length}
    <details>
      <summary class="cursor-pointer py-3 text-sm text-muted-foreground">{$t('bots.recentlyResolved')}</summary>
      {#each resolved as {bot,item} (bot.bot_id + ':' + item.id)}
        <p class="mt-2 text-sm font-semibold">{bot.name}</p>
        {@render card(bot.bot_id,item)}
      {/each}
    </details>
  {/if}
</div>
