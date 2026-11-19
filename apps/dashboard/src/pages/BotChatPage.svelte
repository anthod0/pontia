<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { onMount, tick } from 'svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import { MessageResponse } from '$lib/components/ai-elements/message/index.js'
  import MessageComposer from '../components/chat/MessageComposer.svelte'
  import { parseUploadMessage } from '$lib/uploads/message'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
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

  async function send(content: string): Promise<boolean> {
    const text = draft
    draft = ''
    actionError = ''
    follow = true
    try {
      await device.api.sendBotMessage(botId, content)
      return true
    } catch (failure) {
      draft = draft ? text + '\n\n' + draft : text
      actionError = failure instanceof Error ? failure.message : String(failure)
      return false
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
</script>

{#if chat.data}
  <section class="mx-auto flex h-[calc(100dvh-3rem)] max-w-3xl flex-col" aria-label={chat.data.bot.name}>
    <div class="no-scrollbar min-h-0 flex-1 space-y-5 overflow-y-auto px-1 py-5" bind:this={feed}
      onscroll={(event) => {
        const element = event.currentTarget
        follow = element.scrollHeight - element.scrollTop - element.clientHeight < 64
      }}
      role="log" aria-label={$t('bots.messages')}>
      {#each entries as entry (entry.id)}
        {#if 'content' in entry}
          {@const parts = entry.kind === 'user_input' ? parseUploadMessage(entry.content) : { text: entry.content, files: [] }}
          <div class={['flex', entry.kind === 'user_input' ? 'justify-end' : 'justify-start']}>
            <div class="max-w-[90%] min-w-0 space-y-2">
              {#if parts.files.length}
                <ul class="flex flex-wrap justify-end gap-2" data-user-message-uploads>
                  {#each parts.files as file}<li class="rounded-md border bg-background px-2 py-1 text-xs [overflow-wrap:anywhere]" title={file.path}>{$t('files.uploadedPrefix')} {file.name}</li>{/each}
                </ul>
              {/if}
              {#if parts.text}
                {#if entry.kind === 'bot_reply'}
                  <MessageResponse
                    content={parts.text}
                    markdown
                    class="size-auto rounded-2xl bg-muted px-4 py-3 break-words [&_code]:bg-background [&_th]:bg-background [&_[data-code-block]]:bg-background"
                  />
                {:else}
                  <p class="rounded-2xl border border-border bg-background px-4 py-3 text-sm leading-6 whitespace-pre-wrap [overflow-wrap:anywhere]">{parts.text}</p>
                {/if}
              {/if}
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
        <div aria-hidden="true" class="w-fit rounded-2xl bg-muted px-4 py-3 text-sm text-muted-foreground">
          <span class="animate-pulse">•••</span>
        </div>
      {/if}
    </div>

    <div class="shrink-0 px-1 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
      {#if chat.data.bot.working_on}
        <p class="mb-2 flex items-start gap-2 px-2 text-xs text-muted-foreground" role="status">
          <span class="mt-1 size-1.5 shrink-0 rounded-full bg-primary-solid"></span>
          <span class="min-w-0 [overflow-wrap:anywhere]">{chat.data.bot.working_on}</span>
        </p>
      {/if}
      {#if actionError}<p role="alert" class="mb-2 text-sm text-destructive">{actionError}</p>{/if}
      <MessageComposer draftId={`bot:${botId}`} bind:value={draft} placeholder={$t('bots.message')} fullscreen onSubmit={send} />
    </div>
  </section>
{:else if chat.isError}
  <p role="alert" class="mx-auto max-w-3xl text-sm text-destructive">{chat.error.message}</p>
{:else}
  <p role="status" class="mx-auto max-w-3xl text-sm text-muted-foreground">{$t('common.loading')}</p>
{/if}
