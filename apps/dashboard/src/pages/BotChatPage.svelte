<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from 'svelte'
  import { toast } from 'svelte-sonner'
  import { BotTimeline } from '$lib/botTimeline.svelte'
  import BotMessageFeed from '../components/bots/BotMessageFeed.svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import { MessageResponse } from '$lib/components/ai-elements/message/index.js'
  import SharedFiles from '../components/chat/files/SharedFiles.svelte'
  import MessageComposer from '../components/chat/MessageComposer.svelte'
  import { parseUploadMessage } from '$lib/uploads/message'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon'
  import { useDeviceRuntime } from '../device/context'
  import { openBotChatStream } from '../services/botChatStream'
  import BotAvatar from '../components/bots/BotAvatar.svelte'
  import BotChatSkeleton from '../components/bots/BotChatSkeleton.svelte'
  import { botBubbleClass, highlightableClass, userBubbleClass } from '../components/bots/messageBubble'
  import BotChatPanel from '../components/bots/BotChatPanel.svelte'
  import { provideFilePreviewNavigator } from '../components/chat/filePreviewContext'
  import BotMessageActions from '../components/bots/BotMessageActions.svelte'
  import BotQuoteBlock from '../components/bots/BotQuoteBlock.svelte'
  import { parseQuotedMessage, quoteId, quotedMessage, type BotQuote } from '$lib/botQuote'
  import { canPreviewImage } from '$lib/downloads/preview'
  import { loadQuoteDraft, saveQuoteDraft } from '$lib/botQuoteDraft'

  let { botId }: { botId: string } = $props()
  const device = useDeviceRuntime()

  const timeline = new BotTimeline(device, untrack(() => botId))
  let draft = $state('')
  let typing = $state(false)
  let retrying = $state<string[]>([])
  let feed = $state<BotMessageFeed>()
  let feedRoot = $state<HTMLElement>()
  let actions = $state<BotMessageActions>()
  let panel = $state<BotChatPanel>()
  provideFilePreviewNavigator({ open: file => panel?.openFile(file) })
  let composer = $state<MessageComposer>()
  let quote = $state<BotQuote | null>(loadQuoteDraft(device.id, untrack(() => botId)))
  let highlighted = $state<string | null>(null)
  let jumping = $state<string | null>(null)
  let missingQuotes = $state<string[]>([])
  let highlightTimer: ReturnType<typeof setTimeout> | undefined
  const actionToastId = 'bot-action-error'
  const refreshToastId = 'bot-refresh-error'
  onDestroy(() => {
    clearTimeout(highlightTimer)
    toast.dismiss(actionToastId)
  })

  // The feed stays usable after a failed refresh, so the error stays up until a refresh succeeds.
  $effect(() => {
    const message = timeline.bot ? timeline.refreshError : ''
    if (!message) return
    toast.error(message, {
      id: refreshToastId,
      duration: Number.POSITIVE_INFINITY,
      dismissible: false,
      important: true,
      action: { label: $t('common.retry'), onClick: event => { event.preventDefault(); timeline.refresh(true) } },
    })
    return () => toast.dismiss(refreshToastId)
  })

  function showActionError(failure: unknown, retry: (event: MouseEvent) => void) {
    toast.error(failure instanceof Error ? failure.message : String(failure), {
      id: actionToastId,
      important: true,
      action: { label: $t('common.retry'), onClick: retry },
    })
  }

  function setQuote(next: BotQuote | null) {
    quote = next
    saveQuoteDraft(device.id, botId, next)
  }

  async function addQuote(next: BotQuote) {
    setQuote(next)
    await tick()
    composer?.focusInput()
  }

  async function jumpTo(id: string) {
    if (jumping) return
    jumping = id
    try {
      if (!(await timeline.reveal(id))) {
        missingQuotes = [...missingQuotes, id]
        toast.error($t('bots.quoteMissing'))
        return
      }
      await tick()
      feed?.scrollToEntry(timeline.entryId(id)!)
      highlighted = id
      clearTimeout(highlightTimer)
      highlightTimer = setTimeout(() => { highlighted = null }, 1500)
    } catch (failure) {
      toast.error(failure instanceof Error ? failure.message : String(failure))
    } finally {
      jumping = null
    }
  }

  onMount(() => {
    const stream = openBotChatStream(device, botId, update => {
      typing = update.type === 'typing'
      if (update.type !== 'typing') panel?.refresh()
      if (update.type === 'message') timeline.message(update.message)
      else if (update.type === 'changed' || update.type === 'reset') timeline.refresh(update.type === 'reset')
    }, () => { typing = false })
    const unsubscribeFiles = device.events.subscribeDashboardEvents(event => {
      if (event.kind === 'session_event' && event.event.files) timeline.refresh()
    })
    const resume = () => { if (!document.hidden) timeline.refresh(true) }
    document.addEventListener('visibilitychange', resume)
    window.addEventListener('online', resume)
    timeline.refresh()
    return () => {
      stream()
      unsubscribeFiles()
      timeline.destroy()
      document.removeEventListener('visibilitychange', resume)
      window.removeEventListener('online', resume)
    }
  })
  async function send(content: string): Promise<boolean> {
    const text = draft
    const sentQuote = quote
    draft = ''
    toast.dismiss(actionToastId)
    feed?.followSend()
    try {
      timeline.message(await device.api.sendBotMessage(botId, sentQuote ? quotedMessage(sentQuote, content) : content))
      if (quote === sentQuote) setQuote(null)
      return true
    } catch (failure) {
      draft = draft ? text + '\n\n' + draft : text
      showActionError(failure, event => { if (!composer?.submit()) event.preventDefault() })
      timeline.refresh()
      return false
    }
  }

  const quotableProps = { 'data-bot-quotable': '', tabindex: 0, role: 'article' } as const

  function quoteImage(value: BotQuote) {
    // Reading the entries keeps the thumbnail in step with loaded history.
    if (value.kind !== 'file' || !timeline.entries.length) return null
    const found = timeline.sharedFile(value.fileId)
    return found && canPreviewImage(found.file) ? { sessionId: found.share.session_id, file: found.file } : null
  }

  function quoteSpeaker(value: BotQuote) {
    return value.kind === 'message' && value.speaker === 'User' ? $t('bots.you') : timeline.bot?.name ?? ''
  }

  async function changeTodo(id: string, action: 'resolve' | 'reopen') {
    try {
      const message = await device.api.changeBotTodo(botId, id, action)
      timeline.message(message)
      if (action === 'resolve' && message.todo?.state === 'done' && message.todo.by === 'user') {
        toast.success($t('bots.markedDone'), { id: message.id, action: { label: $t('common.undo'), onClick: () => void changeTodo(id, 'reopen') } })
      }
    } catch (failure) {
      toast.error(failure instanceof Error ? failure.message : String(failure))
      timeline.refresh()
    } finally {
      panel?.refresh()
    }
  }

  async function retry(id: string) {
    retrying = [...retrying, id]
    toast.dismiss(actionToastId)
    try {
      timeline.message(await device.api.retryBotMessage(botId, id))
    } catch (failure) {
      showActionError(failure, () => { if (!retrying.includes(id)) void retry(id) })
      timeline.refresh()
    } finally {
      retrying = retrying.filter((messageId) => messageId !== id)
    }
  }
</script>

{#snippet refreshFailure()}
  <div class="flex items-center justify-center gap-2 text-sm" role="alert">
    {timeline.refreshError}
    <Button size="xs" variant="ghost" onclick={() => timeline.refresh(true)}>{$t('common.retry')}</Button>
  </div>
{/snippet}

<!-- A long press on touch selects text, so touch rows open their menu from this button. -->
{#snippet menuButton(id: string)}
  <Button variant="ghost" size="icon-sm" class="hidden shrink-0 self-end text-muted-foreground pointer-coarse:inline-flex" aria-label={$t('bots.messageActions')}
    onclick={(event: MouseEvent) => actions?.openMenu(id, event.currentTarget as HTMLElement)}><DotsThreeIcon weight="bold" /></Button>
{/snippet}

{#if timeline.bot}
  <BotChatPanel bind:this={panel} bot={timeline.bot} count={timeline.openTodoCount} onjump={jumpTo} onquote={addQuote} ontodochange={changeTodo} />
  <!-- The feed scrolls beneath the transparent top bar, like the Session chat page. -->
  <section class="mx-auto -mt-14 flex h-dvh max-w-3xl flex-col md:-mt-12" aria-label={timeline.bot.name}>
    <BotMessageActions root={feedRoot} entry={id => timeline.entries.find(row => row.id === id)} bind:this={actions} onresolve={id => changeTodo(id, 'resolve')} oncontext={id => panel?.viewContext(id)} onquote={quote => void addQuote(quote)} />
    <BotMessageFeed bind:this={feed} bind:root={feedRoot} {timeline} {typing} highlighted={highlighted && timeline.entries.some(row => row.id === highlighted) ? highlighted : null}>
      {#snippet row(entry)}
        {#if 'content' in entry}
          {@const quoted = entry.kind === 'user_input' ? parseQuotedMessage(entry.content) : { text: entry.content, quote: null }}
          {@const parts = entry.kind === 'user_input' ? { ...parseUploadMessage(quoted.text), quote: quoted.quote } : { ...quoted, files: [] }}
          <div class={['flex gap-1', entry.kind === 'user_input' ? 'justify-end' : 'justify-start']}>
            {#if entry.kind === 'user_input'}{@render menuButton(entry.id)}{/if}
            <div class="max-w-[90%] min-w-0 space-y-2">
              {#if parts.files.length}
                <ul class={['flex flex-wrap justify-end gap-2', !parts.text && highlightableClass]} {...(!parts.text ? quotableProps : {})} data-user-message-uploads>
                  {#each parts.files as file}<li class="rounded-md border bg-background px-2 py-1 text-xs [overflow-wrap:anywhere]" title={file.path}>{$t('files.uploadedPrefix')} {file.name}</li>{/each}
                </ul>
              {/if}
              {#if parts.text}
                {#if entry.kind === 'bot_reply'}
                  <MessageResponse
                    content={parts.text}
                    markdown
                    sessionId={timeline.fileLinkSession?.session_id}
                    linkBase={timeline.fileLinkSession?.workspace ? `${timeline.fileLinkSession.workspace.replace(/\/+$/, '')}/` : null}
                    class="size-auto {botBubbleClass} {highlightableClass} break-words [&_code]:bg-background [&_th]:bg-background [&_[data-code-block]]:bg-background"
                    {...quotableProps}
                    data-bot-quote-source
                  />
                {:else}
                  <div class="{userBubbleClass} {highlightableClass} text-sm leading-6" {...quotableProps}>
                    {#if parts.quote}
                      {@const target = quoteId(parts.quote)}
                      <BotQuoteBlock quote={parts.quote} speaker={quoteSpeaker(parts.quote)} image={quoteImage(parts.quote)} missing={missingQuotes.includes(target)} busy={jumping === target} onjump={() => void jumpTo(target)} />
                    {/if}
                    <p class="whitespace-pre-wrap [overflow-wrap:anywhere]" data-bot-quote-source>{parts.text}</p>
                  </div>
                {/if}
              {/if}
              {#if entry.status === 'failed'}
                <div class="mt-1 flex items-center justify-end gap-2 text-xs text-destructive">
                  <span>{$t('bots.sendFailed')}</span>
                  <Button variant="ghost" size="xs" disabled={retrying.includes(entry.id)} onclick={() => void retry(entry.id)}>{$t('common.retry')}</Button>
                </div>
              {/if}
            </div>
            {#if entry.todo?.state === 'open'}
              <button type="button" class="grid size-7 shrink-0 place-items-center self-end rounded-md outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
                aria-label={$t('bots.todos')} title={$t('bots.todos')} onclick={() => panel?.viewTodos()}>
                <span class="size-2 rounded-full bg-primary"></span>
              </button>
            {/if}
            {#if entry.kind === 'bot_reply'}{@render menuButton(entry.id)}{/if}
          </div>
        {:else if entry.kind === 'shared_files'}
          <div class="max-w-[90%]">
            <SharedFiles files={entry.files} sessionId={entry.session_id} highlighted={highlighted}
              itemProps={file => ({ ...quotableProps, 'data-bot-file-id': file.file_id })}>
              {#snippet itemActions()}{@render menuButton(entry.id)}{/snippet}
            </SharedFiles>
          </div>
        {:else}
          <p class="flex items-center justify-center gap-2 text-center text-xs text-muted-foreground">
            <WarningCircleIcon class="size-4 shrink-0" />
            {entry.kind === 'session.exited' ? $t('bots.sessionExited') : $t('bots.executionFailed')}
          </p>
        {/if}
      {/snippet}
    </BotMessageFeed>
    <div class="shrink-0 px-1 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
      {#if timeline.status.kind === 'processing'}
        {#if !typing}
          <p class="mb-2 flex items-start gap-2 px-2 text-sm leading-5 text-muted-foreground" role="status">
            <BotAvatar shape={timeline.bot.avatar_shape} color={timeline.bot.avatar_color} working class="size-5 shrink-0" />
            <span class="shimmer min-w-0">{$t('bots.processing')}</span>
          </p>
        {/if}
      {:else if timeline.status.kind === 'working'}
        <p class="mb-2 flex items-start gap-2 px-2 text-sm leading-5 text-muted-foreground" role="status">
          <BotAvatar shape={timeline.bot.avatar_shape} color={timeline.bot.avatar_color} working class="size-5 shrink-0" />
          <span class="shimmer line-clamp-2 min-w-0 [overflow-wrap:anywhere]" title={timeline.status.title}>{timeline.status.title}</span>
        </p>
      {/if}
      <div aria-live="polite">
        {#if quote}<BotQuoteBlock {quote} speaker={quoteSpeaker(quote)} image={quoteImage(quote)} onremove={() => setQuote(null)} />{/if}
      </div>
      <MessageComposer bind:this={composer} draftId={`bot:${botId}`} bind:value={draft} placeholder={$t('bots.message')} fullscreen
        submitDisabled={quote !== null && !draft.trim()}
        onSubmit={send} />
    </div>
  </section>
{:else if timeline.refreshError}
  {@render refreshFailure()}
{:else}
  <BotChatSkeleton />
{/if}
