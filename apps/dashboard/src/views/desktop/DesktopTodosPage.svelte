<script lang="ts">
  import { t } from '$lib/i18n'
  import { dashboardPath } from '$lib/navigation'
  import { createBotDirectory } from '../../features/bots/botDirectory.svelte'
  import { createBotTodos } from '../../features/bots/botTodos.svelte'
  import { useDeviceRuntime } from '../../device/context'

  const device = useDeviceRuntime()
  const directory = createBotDirectory(device)
  const todos = createBotTodos(device, directory)
</script>

<section class="mx-auto grid max-w-4xl gap-5">
  <h1 class="text-2xl font-semibold">{$t('bots.todos')}</h1>
  {#each todos.groups as {bot, result} (bot.bot_id)}
    <section class="grid gap-2">
      <h2 class="font-semibold"><a href={dashboardPath('/bots/' + encodeURIComponent(bot.bot_id))}>{bot.name}</a></h2>
      {#if result.error}
        <p role="alert">{result.error.message}</p>
      {:else if result.isPending}
        <p role="status">{$t('common.loading')}</p>
      {:else}
        <ul class="divide-y rounded-lg border bg-card">
          {#each result.data?.open ?? [] as item (item.id)}
            <li class="whitespace-pre-wrap break-words px-4 py-3">{item.content}</li>
          {/each}
        </ul>
      {/if}
    </section>
  {:else}
    <p role="status" class="text-muted-foreground">{directory.error?.message ?? $t(directory.pending ? 'common.loading' : 'bots.noTodos')}</p>
  {/each}
</section>
