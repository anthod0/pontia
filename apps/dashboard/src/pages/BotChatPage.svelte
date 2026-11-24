<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from 'svelte'
  import { toast } from 'svelte-sonner'
  import { BotTimeline } from '$lib/botTimeline.svelte'
  import BotMessageFeed from '../components/bots/BotMessageFeed.svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import { MessageResponse } from '$lib/components/ai-elements/message/index.js'
  import SharedFiles from '../components/chat/files/SharedFiles.svelte'
  import { fileActionButton, fileActionClass, type FileActionLook } from '../components/chat/files/fileActionButton'
  import { ApiError } from '../api/errors'
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
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { parseQuotedMessage, quotedMessage, type BotQuote } from '$lib/botQuote'
  import { splitAttachments } from '$lib/attachmentBlocks'
  import { isImagePath } from '$lib/downloads/localFilePath'
  import { createQuoteDraft } from '$lib/botQuoteDraft'

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
  const quoteDraft = createQuoteDraft(device.id, untrack(() => botId))
  const quote = $derived(quoteDraft.current)
  let highlighted = $state<string | null>(null)
  // The attached file to mark within the highlighted message.
  let highlightedPath = $state<string | null>(null)
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
    quoteDraft.current = next
  }

  async function addQuote(next: BotQuote) {
    setQuote(next)
    await tick()
    composer?.focusInput()
  }

  async function jumpTo(id: string, path: string | null = null) {
    if (jumping) return
    jumping = id
    try {
      if (!(await timeline.reveal(id))) {
        missingQuotes = [...missingQuotes, id]
        toast.error($t('bots.quoteMissing'))
        return
      }
      await tick()
      feed?.scrollToEntry(id)
      highlighted = id
      highlightedPath = path
      clearTimeout(highlightTimer)
      highlightTimer = setTimeout(() => { highlighted = highlightedPath = null }, 1500)
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
    const resume = () => { if (!document.hidden) timeline.refresh(true) }
    document.addEventListener('visibilitychange', resume)
    window.addEventListener('online', resume)
    timeline.refresh()
    return () => {
      stream()
      timeline.destroy()
      document.removeEventListener('visibilitychange', resume)
      window.removeEventListener('online', resume)
    }
  })
  function showDelivery(message: Awaited<ReturnType<typeof device.api.sendBotMessage>>) {
    timeline.message(message)
    if (message.delivery === 'queued') toast.info($t('bots.queuedAfterTurn'))
  }
  // The composer and a todo's reply box both send through here, so a reply matches quoting first and sending from the composer.
  async function post(content: string, sentQuote: BotQuote | null) {
    toast.dismiss(actionToastId)
    feed?.followSend()
    showDelivery(await device.api.sendBotMessage(botId, sentQuote ? quotedMessage(sentQuote, content) : content))
  }

  let reloading = $state(false)
  async function reload() {
    if (reloading) return
    reloading = true
    try {
      await device.api.reloadBot(botId)
      if (draft.trim() === '/reload') draft = ''
      toast.success($t('bots.reloadReady'))
    } catch (error) {
      toast.error(error instanceof ApiError && error.code === 'state_conflict'
        ? $t('bots.reloadBusy')
        : error instanceof Error ? error.message : String(error))
    } finally {
      reloading = false
    }
  }

  async function send(content: string): Promise<boolean> {
    const text = draft
    const sentQuote = quote
    draft = ''
    try {
      await post(content, sentQuote)
      if (quote === sentQuote) setQuote(null)
      return true
    } catch (failure) {
      draft = draft ? text + '\n\n' + draft : text
      showActionError(failure, event => { if (!composer?.submit()) event.preventDefault() })
      timeline.refresh()
      return false
    }
  }

  async function replyTodo(todo: BotQuote, text: string, retry: () => boolean): Promise<boolean> {
    try {
      await post(text, todo)
      return true
    } catch (failure) {
      showActionError(failure, event => { if (!retry()) event.preventDefault() })
      timeline.refresh()
      return false
    }
  }

  const quotableProps = { 'data-bot-quotable': '', tabindex: 0, role: 'article' } as const

  function quoteImage(value: BotQuote) {
    const sessionId = timeline.fileLinkSession?.session_id
    return value.kind === 'file' && sessionId && isImagePath(value.path) ? { sessionId, path: value.path } : null
  }

  function quoteSpeaker(value: BotQuote) {
    return value.kind === 'message' && value.speaker === 'User' ? $t('bots.you') : timeline.bot?.name ?? ''
  }

  async function changeTodo(id: string, action: 'resolve' | 'reopen') {
    try {
      const message = await device.api.changeBotTodo(botId, id, action)
      timeline.message(message)
      if (action === 'resolve' && message.todo?.state === 'done' && message.todo.by === 'user') {
        toast.success($t('bots.ignored'), { id: message.id, action: { label: $t('common.undo'), onClick: () => void changeTodo(id, 'reopen') } })
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
      showDelivery(await device.api.retryBotMessage(botId, id))
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

<!-- A long press on touch selects text, so touch rows always show this button.
     With a mouse it appears while its message is hovered or keyboard-focused, and while its menu is open. -->
{#snippet menuButton(id: string)}
  <Button variant="ghost" size="icon-sm" aria-haspopup="menu" aria-label={$t('bots.messageActions')}
    class="shrink-0 self-end text-muted-foreground opacity-0 transition-opacity focus-visible:opacity-100 aria-expanded:opacity-100 pointer-coarse:opacity-100 group-hover/entry:opacity-100 group-has-focus-visible/entry:opacity-100"
    onclick={(event: MouseEvent) => actions?.openMenu(id, event)}><DotsThreeIcon weight="bold" /></Button>
{/snippet}

<!-- A file's menu looks like its download action and always shows; image tiles keep their own reveal. -->
{#snippet fileMenuButton(id: string, look: FileActionLook)}
  <Button {...fileActionButton} class={fileActionClass[look]} aria-haspopup="menu" aria-label={$t('bots.messageActions')}
    onclick={(event: MouseEvent) => actions?.openMenu(id, event)}><DotsThreeIcon weight="bold" /></Button>
{/snippet}

{#if timeline.bot}
  <BotChatPanel bind:this={panel} bot={timeline.bot} count={timeline.openTodoCount} onjump={jumpTo} onquote={addQuote} onreply={replyTodo} ontodochange={changeTodo} />
  <!-- The feed scrolls beneath the transparent top bar, like the Session chat page. -->
  <section class="mx-auto -mt-14 flex h-dvh max-w-3xl flex-col md:-mt-12" aria-label={timeline.bot.name}>
    <BotMessageActions root={feedRoot} fileSessionId={timeline.fileLinkSession?.session_id} entry={id => timeline.entries.find(row => row.id === id)} bind:this={actions} onresolve={id => changeTodo(id, 'resolve')} oncontext={id => panel?.viewContext(id)} onquote={quote => void addQuote(quote)} />
    <BotMessageFeed bind:this={feed} bind:root={feedRoot} {timeline} {typing} highlighted={highlighted && !highlightedPath && timeline.entries.some(row => row.id === highlighted) ? highlighted : null}>
      {#snippet row(entry)}
        {#if 'content' in entry}
          {@const quoted = entry.kind === 'user_input' ? parseQuotedMessage(entry.content) : { text: entry.content, quote: null }}
          {@const parts = entry.kind === 'user_input' ? { ...parseUploadMessage(quoted.text), quote: quoted.quote } : { ...quoted, files: [] }}
          {@const fileSession = timeline.fileLinkSession?.session_id}
          <div class={['flex gap-1', entry.kind === 'user_input' ? 'justify-end' : 'justify-start']}>
            {#if entry.kind === 'user_input'}{@render menuButton(entry.id)}{/if}
            <div class="max-w-[90%] min-w-0 space-y-2">
              {#if parts.files.length}
                <ul class={['flex flex-wrap justify-end gap-2', !parts.text && highlightableClass]} {...(!parts.text ? quotableProps : {})} data-user-message-uploads>
                  {#each parts.files as file}<li class="rounded-md border bg-background px-2 py-1 text-xs [overflow-wrap:anywhere]" title={file.path}>{$t('files.uploadedPrefix')} {file.name}</li>{/each}
                </ul>
              {/if}
              {#if entry.kind === 'bot_reply'}
                {#each splitAttachments(entry.content) as part}
                  {#if part.kind === 'text'}
                    <MessageResponse
                      content={part.text}
                      markdown
                      sessionId={timeline.fileLinkSession?.session_id}
                      linkBase={timeline.fileLinkSession?.workspace ? `${timeline.fileLinkSession.workspace.replace(/\/+$/, '')}/` : null}
                      class="size-auto {botBubbleClass} {highlightableClass} break-words [&_code]:bg-background [&_th]:bg-background [&_[data-code-block]]:bg-background"
                      {...quotableProps}
                      data-bot-quote-source
                    />
                  {:else if fileSession}
                    <SharedFiles paths={part.paths} sessionId={fileSession} highlighted={highlighted === entry.id ? highlightedPath : null}
                      itemProps={file => ({ ...quotableProps, 'data-bot-file-path': file.path })}>
                      {#snippet itemActions(look: FileActionLook)}{@render fileMenuButton(entry.id, look)}{/snippet}
                    </SharedFiles>
                  {/if}
                {/each}
              {:else if parts.text}
                <div class="{userBubbleClass} {highlightableClass} text-sm leading-6" {...quotableProps}>
                  {#if parts.quote}
                    {@const current = parts.quote}
                    {@const target = current.messageId}
                    <BotQuoteBlock quote={current} speaker={quoteSpeaker(current)} image={quoteImage(current)} missing={missingQuotes.includes(target)} busy={jumping === target} onjump={() => void jumpTo(target, current.kind === 'file' ? current.path : null)} />
                  {/if}
                  <p class="whitespace-pre-wrap [overflow-wrap:anywhere]" data-bot-quote-source>{parts.text}</p>
                </div>
              {/if}
              <div class={['-mt-1 flex items-center gap-2 text-xs text-muted-foreground', entry.kind === 'user_input' ? 'justify-end' : 'justify-start']} data-bot-message-meta>
                {#if entry.status === 'failed'}
                  <span class="flex items-center gap-2 text-destructive">
                    {$t('bots.sendFailed')}
                    <Button variant="ghost" size="xs" disabled={retrying.includes(entry.id)} onclick={() => void retry(entry.id)}>{$t('common.retry')}</Button>
                  </span>
                {/if}
                <MessageTime value={entry.created_at} />
              </div>
            </div>
            {#if entry.todo?.state === 'open'}
              <button type="button" class="grid size-7 shrink-0 place-items-center self-end rounded-md outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
                aria-label={$t('bots.todos')} title={$t('bots.todos')} onclick={() => panel?.viewTodos()}>
                <span class="size-2 rounded-full bg-primary"></span>
              </button>
            {/if}
            {#if entry.kind === 'bot_reply'}{@render menuButton(entry.id)}{/if}
          </div>
        {:else}
          <p class="flex items-center justify-center gap-2 text-center text-xs text-muted-foreground">
            <WarningCircleIcon class="size-4 shrink-0" />
            {entry.kind === 'session.exited' ? $t('bots.sessionExited') : $t('bots.executionFailed')}
            <MessageTime value={entry.created_at} class="px-0" />
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
        busy={reloading} commands={[{ name: '/reload', description: $t('bots.reload'), run: reload }]}
        onSubmit={send} />
    </div>
  </section>
{:else if timeline.refreshError}
  {@render refreshFailure()}
{:else}
  <BotChatSkeleton />
{/if}
