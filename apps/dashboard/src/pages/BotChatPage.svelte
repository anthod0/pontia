<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { onMount, tick } from 'svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon'
  import GearIcon from 'phosphor-svelte/lib/GearIcon'
  import RobotIcon from 'phosphor-svelte/lib/RobotIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import BotSettingsDialog from '../components/bots/BotSettingsDialog.svelte'
  import { useDeviceRuntime } from '../device/context'
  import { openBotChatStream } from '../services/botChatStream'

  let { botId }: { botId: string } = $props()
  const device = useDeviceRuntime()
  const chat = createQuery(
    () => ({
      queryKey: ['bot-chat', botId],
      queryFn: ({ signal }) => device.api.getBotChat(botId, signal),
    }),
    () => device.queryClient,
  )

  const entries = $derived([...(chat.data?.messages ?? []), ...(chat.data?.notices ?? [])]
    .sort((a, b) => Date.parse(a.created_at) - Date.parse(b.created_at) || a.id.localeCompare(b.id)))
  let draft = $state('')
  let typing = $state(false)
  let editing = $state(false)
  let actionError = $state('')
  let retrying = $state<string[]>([])
  let feed = $state<HTMLDivElement>()
  let follow = true
  let lastStreamUpdate = 0

  onMount(() => openBotChatStream(device, botId, (update) => {
    const sequence = ++lastStreamUpdate
    if (update === 'typing') {
      typing = true
    } else {
      void refresh().finally(() => {
        if (sequence === lastStreamUpdate) typing = false
      })
    }
  }))

  $effect(() => {
    if ((entries.length || typing) && follow) {
      void tick().then(() => {
        if (feed && follow) feed.scrollTop = feed.scrollHeight
      })
    }
  })

  async function refresh() {
    await device.queryClient.invalidateQueries({ queryKey: ['bot-chat', botId] })
  }

  async function send() {
    const content = draft.trim()
    if (!content) return
    draft = ''
    actionError = ''
    follow = true
    try {
      await device.api.sendBotMessage(botId, content)
    } catch (failure) {
      draft = draft ? content + '\n\n' + draft : content
      actionError = failure instanceof Error ? failure.message : String(failure)
    } finally {
      await refresh()
    }
  }

  async function retry(id: string) {
    retrying = [...retrying, id]
    actionError = ''
    try {
      await device.api.retryBotMessage(botId, id)
    } catch (failure) {
      actionError = failure instanceof Error ? failure.message : String(failure)
    } finally {
      retrying = retrying.filter((messageId) => messageId !== id)
      await refresh()
    }
  }

  function onComposerKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault()
      void send()
    }
  }
</script>

{#if chat.data}
  <section class="mx-auto flex h-[calc(100dvh-3.5rem)] max-w-3xl flex-col" aria-label={chat.data.bot.name}>
    <header class="flex shrink-0 items-start gap-3 border-b border-border px-1 pb-4">
      <div class="grid size-10 shrink-0 place-items-center rounded-lg bg-muted text-foreground">
        <RobotIcon class="size-5" />
      </div>
      <div class="min-w-0 flex-1">
        <h1 class="break-words text-lg font-semibold text-heading">{chat.data.bot.name}</h1>
        {#if chat.data.bot.description}<p class="break-words text-sm text-muted-foreground">{chat.data.bot.description}</p>{/if}
        <p class="mt-1 flex items-start gap-2 text-xs text-muted-foreground" role="status">
          <span class={['mt-1 size-1.5 shrink-0 rounded-full', typing ? 'animate-pulse bg-primary-solid' : chat.data.bot.working_on ? 'bg-primary-solid' : 'bg-muted-foreground/50']}></span>
          {typing ? $t('bots.typing') : chat.data.bot.working_on ?? $t('bots.idle')}
        </p>
      </div>
      <Button variant="ghost" size="icon" aria-label={$t('bots.edit')} title={$t('bots.edit')} onclick={() => { editing = true }}>
        <GearIcon class="size-5" />
      </Button>
    </header>

    <div class="min-h-0 flex-1 space-y-5 overflow-y-auto px-1 py-5" bind:this={feed}
      onscroll={(event) => {
        const element = event.currentTarget
        follow = element.scrollHeight - element.scrollTop - element.clientHeight < 64
      }}
      role="log" aria-label={$t('bots.messages')}>
      {#each entries as entry (entry.id)}
        {#if 'content' in entry}
          <div class={['flex', entry.kind === 'user_input' ? 'justify-end' : 'justify-start']}>
            <div class="max-w-[90%] min-w-0">
              <p class={['rounded-2xl px-4 py-3 text-sm leading-6 whitespace-pre-wrap [overflow-wrap:anywhere]', entry.kind === 'user_input' ? 'bg-muted' : 'border border-border bg-background']}>{entry.content}</p>
              {#if entry.status === 'failed'}
                <div class="mt-1 flex items-center justify-end gap-2 text-xs text-destructive">
                  <span>{$t('bots.sendFailed')}</span>
                  <Button variant="ghost" size="xs" disabled={retrying.includes(entry.id)} onclick={() => void retry(entry.id)}>{$t('common.retry')}</Button>
                </div>
              {/if}
            </div>
          </div>
        {:else}
          <p class="flex items-center justify-center gap-2 text-center text-xs text-muted-foreground">
            <WarningCircleIcon class="size-4 shrink-0" />
            {entry.kind === 'session.exited' ? $t('bots.sessionExited') : $t('bots.executionFailed')}
          </p>
        {/if}
      {/each}
      {#if typing}
        <div aria-hidden="true" class="w-fit rounded-2xl border border-border bg-background px-4 py-3 text-sm text-muted-foreground">
          <span class="animate-pulse">•••</span>
        </div>
      {/if}
    </div>

    <form class="shrink-0 px-1 pt-2 pb-4" onsubmit={(event) => { event.preventDefault(); void send() }}>
      {#if actionError}<p role="alert" class="mb-2 text-sm text-destructive">{actionError}</p>{/if}
      <div class="flex items-end gap-2 rounded-xl border border-border bg-background p-2">
        <Textarea rows={2} bind:value={draft} onkeydown={onComposerKey} aria-label={$t('bots.message')}
          placeholder={$t('bots.message')} class="min-h-14 flex-1 resize-none border-0 shadow-none focus-visible:ring-0" />
        <Button size="icon" type="submit" disabled={!draft.trim()} aria-label={$t('common.send')} title={$t('common.send')}>
          <ArrowUpIcon class="size-5" />
        </Button>
      </div>
    </form>
  </section>

  {#if editing}
    <BotSettingsDialog bot={chat.data.bot} onclose={() => { editing = false }} onsaved={() => {
      editing = false
      void refresh()
    }} />
  {/if}
{:else if chat.isError}
  <p role="alert" class="mx-auto max-w-3xl text-sm text-destructive">{chat.error.message}</p>
{:else}
  <p role="status" class="mx-auto max-w-3xl text-sm text-muted-foreground">{$t('common.loading')}</p>
{/if}
