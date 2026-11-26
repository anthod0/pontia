<script lang="ts">
  import { onDestroy, tick } from 'svelte'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import CaretUpIcon from 'phosphor-svelte/lib/CaretUpIcon'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import CrosshairSimpleIcon from 'phosphor-svelte/lib/CrosshairSimpleIcon'
  import InfoIcon from 'phosphor-svelte/lib/InfoIcon'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import type { Bot, BotMessage } from '../../api/types'
  import { splitAttachments } from '$lib/attachmentBlocks'
  import { parseQuotedMessage, type BotQuote } from '$lib/botQuote'
  import { isImagePath } from '$lib/downloads/localFilePath'
  import { t } from '$lib/i18n'
  import { parseUploadMessage } from '$lib/uploads/message'
  import { MessageResponse } from '$lib/components/ai-elements/message/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Tooltip from '$lib/components/ui/tooltip/index.js'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import SentSecrets from '../chat/SentSecrets.svelte'
  import SharedFiles from '../chat/files/SharedFiles.svelte'
  import UploadedFiles from '../chat/files/UploadedFiles.svelte'
  import BotAvatar from './BotAvatar.svelte'
  import BotQuoteBlock from './BotQuoteBlock.svelte'

  type FileLinkSession = { session_id: string; workspace: string | null } | null

  let {
    bot,
    selected,
    messages = null,
    truncated = false,
    error = null,
    mobile = false,
    fullscreen = false,
    fileLinkSession = null,
    onretry,
    onjump,
  }: {
    bot: Bot
    selected: string
    messages?: BotMessage[] | null
    truncated?: boolean
    error?: Error | null
    mobile?: boolean
    fullscreen?: boolean
    fileLinkSession?: FileLinkSession
    onretry?: () => void
    onjump: (id: string, path?: string | null) => void | Promise<void>
  } = $props()

  let scroller = $state<HTMLElement>()
  let open = $state<Set<string>>(new Set())
  let clamped = $state<Set<string>>(new Set())
  let flashed = $state<string | null>(null)
  const bodies = new Map<string, HTMLElement>()
  let flashTimer: ReturnType<typeof setTimeout> | null = null

  const linkBase = $derived(fileLinkSession?.workspace ? `${fileLinkSession.workspace.replace(/\/+$/, '')}/` : null)
  const range = $derived(messages?.length ? `${formatRangeTime(messages[0].created_at)} – ${formatRangeTime(messages.at(-1)!.created_at)}` : '')

  onDestroy(() => {
    if (flashTimer) clearTimeout(flashTimer)
  })

  $effect(() => reset(selected, messages))

  function reset(_selected: string, _messages: BotMessage[] | null) {
    open = new Set()
    clamped = new Set()
    void tick().then(() => {
      measureAll()
      requestAnimationFrame(scrollToCurrent)
    })
  }

  function formatRangeTime(value: string) {
    return new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(new Date(value))
  }

  function observeBody(id: string, node: HTMLElement) {
    bodies.set(id, node)
    const observer = new ResizeObserver(() => measure(id, node))
    observer.observe(node)
    void tick().then(() => measure(id, node))
    return () => {
      observer.disconnect()
      bodies.delete(id)
    }
  }

  function measureAll() {
    for (const [id, node] of bodies) measure(id, node)
  }

  function measure(id: string, node: HTMLElement) {
    const lineHeight = Number.parseFloat(getComputedStyle(node).lineHeight)
    const shouldClamp = id !== selected && node.scrollHeight > lineHeight * 5 + 1
    if (clamped.has(id) === shouldClamp) return
    const next = new Set(clamped)
    if (shouldClamp) next.add(id)
    else next.delete(id)
    clamped = next
  }

  function scrollToCurrent() {
    const current = scroller?.querySelector<HTMLElement>(`[data-context-message="${CSS.escape(selected)}"]`)
    if (!current || !scroller) return
    const sticky = scroller.querySelector<HTMLElement>('[data-context-summary]')?.offsetHeight ?? 0
    const available = scroller.clientHeight - sticky
    const currentTop = current.offsetTop
    const currentBottom = currentTop + current.offsetHeight
    if (current.offsetHeight > available) {
      scroller.scrollTop = Math.max(0, currentTop - sticky)
      return
    }
    const centered = Math.max(0, currentTop - sticky - (available - current.offsetHeight) / 2)
    const previous = current.previousElementSibling as HTMLElement | null
    if (!previous || previous.offsetTop + previous.offsetHeight <= centered + sticky) {
      scroller.scrollTop = centered
      return
    }
    const snapped = Math.max(0, previous.offsetTop - sticky)
    scroller.scrollTop = snapped + scroller.clientHeight >= currentBottom ? snapped : centered
  }

  function toggle(id: string, value: boolean) {
    const next = new Set(open)
    if (value) next.add(id)
    else next.delete(id)
    open = next
    if (!value) {
      void tick().then(() => {
        const item = scroller?.querySelector<HTMLElement>(`[data-context-message="${CSS.escape(id)}"]`)
        const sticky = scroller?.querySelector<HTMLElement>('[data-context-summary]')?.offsetHeight ?? 0
        if (item && scroller && item.getBoundingClientRect().top < scroller.getBoundingClientRect().top + sticky) {
          item.scrollIntoView({ behavior: 'smooth', block: 'start' })
        }
      })
    }
  }

  function isInteractive(target: EventTarget | null) {
    return target instanceof Element && Boolean(target.closest('button, a, input, textarea, select, [role="button"]'))
  }

  function itemClick(event: MouseEvent, id: string) {
    if (!clamped.has(id) || open.has(id) || isInteractive(event.target) || window.getSelection()?.isCollapsed === false) return
    toggle(id, true)
  }

  function headerClick(event: MouseEvent, id: string) {
    if (!open.has(id) || !clamped.has(id) || isInteractive(event.target)) return
    event.stopPropagation()
    toggle(id, false)
  }

  async function quoteJump(quote: BotQuote) {
    const target = messages?.find(message => message.id === quote.messageId)
    if (!target) {
      await onjump(quote.messageId, quote.kind === 'file' ? quote.path : null)
      return
    }
    toggle(target.id, true)
    await tick()
    scroller?.querySelector<HTMLElement>(`[data-context-message="${CSS.escape(target.id)}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    flashed = target.id
    if (flashTimer) clearTimeout(flashTimer)
    flashTimer = setTimeout(() => {
      flashed = null
      flashTimer = null
    }, 1800)
  }

  function quoteSpeaker(quote: BotQuote) {
    return quote.kind === 'message' && quote.speaker === 'User' ? $t('bots.you') : bot.name
  }

  function quoteImage(quote: BotQuote) {
    return quote.kind === 'file' && fileLinkSession && isImagePath(quote.path)
      ? { sessionId: fileLinkSession.session_id, path: quote.path }
      : null
  }
</script>

{#snippet loadingRows()}
  <div role="status" aria-label={$t('common.loading')} class="px-3 py-1">
    {#each [0, 1, 2, 3] as row}
      <div class="grid grid-cols-[1.5rem_minmax(0,1fr)] gap-3 py-3.5">
        <Skeleton class="size-6 rounded-full" />
        <div class="space-y-2.5 pt-1"><Skeleton class="h-3 w-1/3" /><Skeleton class="h-3 w-full" /><Skeleton class={['h-3', row % 2 ? 'w-3/4' : 'w-2/3']} /></div>
      </div>
    {/each}
  </div>
{/snippet}

<Tooltip.Provider>
  <div
    bind:this={scroller}
    class={[
      'min-h-0 flex-1 overflow-auto overscroll-contain',
      mobile ? 'bg-background' : 'bg-surface',
    ]}
    data-bot-context-list
    data-mobile={mobile || undefined}
    data-fullscreen={fullscreen || undefined}
  >
  {#if error}
    <Empty.Root class="min-h-full p-8" role="alert">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><WarningCircleIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('bots.contextLoadFailed')}</Empty.Title>
        <Empty.Description>{error.message}</Empty.Description>
      </Empty.Header>
      {#if onretry}<Button variant="outline" size="sm" onclick={onretry}><ArrowClockwiseIcon />{$t('common.retry')}</Button>{/if}
    </Empty.Root>
  {:else if !messages}
    {@render loadingRows()}
  {:else}
    <div class={['mx-auto pb-8', fullscreen && 'max-w-[848px] pb-16']}>
      <div class={['sticky top-0 z-20 bg-surface px-2.5', mobile && 'hidden bg-background', fullscreen && 'px-6']} data-context-summary>
        <div class={['flex items-center gap-2 px-3 pt-3 pb-2 text-[13px] text-muted-foreground', fullscreen && 'text-sm']}>
          <TreeStructureIcon class="size-4" />
          <span>{$t('bots.contextCount', { count: messages.length })}{#if range} · {range}{/if}</span>
        </div>
        {#if truncated}
          <div class="mb-2 flex items-start gap-2 rounded-[10px] bg-muted px-3 py-2 text-[13px] leading-[1.55] text-muted-foreground">
            <InfoIcon class="mt-0.5 size-4 shrink-0" /><span>{$t('bots.contextTruncated')}</span>
          </div>
        {/if}
      </div>
      {#if mobile && truncated}
        <div class="mx-4 mt-2 flex items-start gap-2 rounded-[10px] bg-muted px-3 py-2 text-[13px] leading-[1.55] text-muted-foreground">
          <InfoIcon class="mt-0.5 size-4 shrink-0" /><span>{$t('bots.contextTruncated')}</span>
        </div>
      {/if}
      <ol class={['m-0 list-none p-0', mobile && 'pt-1']}>
        {#each messages as message (message.id)}
          {@const current = message.id === selected}
          {@const user = message.kind === 'user_input'}
          {@const quoted = user ? parseQuotedMessage(message.content) : null}
          {@const uploaded = quoted ? parseUploadMessage(quoted.text) : null}
          {@const expanded = open.has(message.id)}
          {@const isClamped = clamped.has(message.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <li
            data-context-message={message.id}
            aria-current={current ? 'true' : undefined}
            class={[
              'group/context relative -mx-0 px-[22px] py-3.5 transition-colors',
              fullscreen && 'px-9',
              mobile && 'px-7',
              current && 'bg-selected before:absolute before:inset-y-0 before:left-0 before:w-[3px] before:bg-primary',
              isClamped && !expanded && 'cursor-pointer hover:bg-muted/75 dark:hover:bg-muted/60',
              flashed === message.id && 'context-message-flash',
            ]}
            onclick={(event) => itemClick(event, message.id)}
          >
            {#if message !== messages.at(-1)}<span aria-hidden="true" class={['absolute top-11 -bottom-2 left-[33.5px] w-px bg-border', fullscreen && 'left-[49.5px]', mobile && 'left-[39.5px]']}></span>{/if}
            <article class={['relative grid grid-cols-[1.5rem_minmax(0,1fr)] gap-3', fullscreen && 'grid-cols-[1.75rem_minmax(0,1fr)]']} aria-label={`${user ? $t('bots.you') : bot.name} ${message.created_at}`}>
              <span class={['relative z-10 grid size-6 shrink-0 place-items-center rounded-full', fullscreen && 'size-7', current ? 'bg-selected' : mobile ? 'bg-background' : 'bg-surface']}>
                {#if user}<span aria-hidden="true" class={['grid size-6 place-items-center rounded-full bg-border text-[11px] font-semibold text-heading', fullscreen && 'size-7 text-xs']}>{$t('bots.you').slice(0, 1)}</span>
                {:else}<BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} class={fullscreen ? 'size-7' : 'size-6'} />{/if}
              </span>
              <div class="min-w-0">
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <header class={['flex min-h-6 items-center gap-2 text-sm', expanded && isClamped && 'cursor-pointer']} onclick={(event) => headerClick(event, message.id)}>
                  <span class="min-w-0 font-semibold text-heading [overflow-wrap:anywhere]">{user ? $t('bots.you') : bot.name}</span>
                  <MessageTime value={message.created_at} class="px-0 text-[13px] whitespace-nowrap tabular-nums" />
                  {#if current}<Badge variant="secondary" class="h-5 rounded-md bg-primary/14 px-1.5 text-xs font-medium text-primary">{$t('bots.contextCurrent')}</Badge>{/if}
                  {#if message.todo?.state === 'open'}<Badge variant="secondary" class="h-5 rounded-md bg-primary/14 px-1.5 text-xs font-medium text-primary">{$t('bots.todos')}</Badge>
                  {:else if message.todo?.state === 'done'}<span class="inline-flex items-center gap-1 text-xs text-muted-foreground"><CheckCircleIcon class="size-3" />{$t('bots.todoDone')}</span>{/if}
                  {#if message.status === 'failed'}<Badge variant="secondary" class="h-5 rounded-md bg-destructive/12 px-1.5 text-xs font-medium text-destructive">{$t('bots.sendFailed')}</Badge>{/if}
                  <Tooltip.Root>
                    <Tooltip.Trigger>
                      {#snippet child({ props })}
                        <Button
                          {...props}
                          variant="ghost"
                          size="sm"
                          class={[
                            'ml-auto h-7 shrink-0 gap-1 rounded-full px-2.5 text-[13px] font-normal text-muted-foreground touch-target:h-11 touch-target:px-3 touch-target:text-sm hover:text-heading',
                            current && 'hover:bg-primary/14',
                          ]}
                          aria-label={$t('bots.locateInChat', { name: user ? $t('bots.you') : bot.name, time: formatRangeTime(message.created_at) })}
                          onclick={() => onjump(message.id)}
                        ><CrosshairSimpleIcon class="size-[15px]" />{$t('bots.locate')}</Button>
                      {/snippet}
                    </Tooltip.Trigger>
                    <Tooltip.Content>{$t('bots.locateInChat', { name: user ? $t('bots.you') : bot.name, time: formatRangeTime(message.created_at) })}</Tooltip.Content>
                  </Tooltip.Root>
                </header>
                <div
                  id={`bot-context-body-${message.id}`}
                  class={[
                    'mt-1 text-[15px] leading-[1.75] text-foreground [overflow-wrap:anywhere]',
                    (fullscreen || mobile) && 'text-base',
                    fullscreen && 'leading-[1.8]',
                    mobile && 'leading-[1.7]',
                    isClamped && !expanded && 'context-body-clamped',
                    current && '[&_code]:bg-background [&_th]:bg-background [&_[data-code-block]]:bg-background',
                  ]}
                  {@attach node => observeBody(message.id, node)}
                >
                  {#if user && uploaded}
                    {#if uploaded.secrets.length}<SentSecrets secrets={uploaded.secrets} class="mb-2 justify-start" />{/if}
                    {#if uploaded.files.length}<UploadedFiles files={uploaded.files} class="mb-2 ml-0" />{/if}
                    {#if quoted?.quote}
                      {@const quote = quoted.quote}
                      <BotQuoteBlock quote={quote} speaker={quoteSpeaker(quote)} image={quoteImage(quote)} onjump={() => void quoteJump(quote)} />
                    {/if}
                    {#if uploaded.text}<p class="whitespace-pre-wrap [overflow-wrap:anywhere]">{uploaded.text}</p>{/if}
                  {:else if !user}
                    {#each splitAttachments(message.content) as part}
                      {#if part.kind === 'text'}
                        <MessageResponse content={part.text} markdown variant="context" sessionId={fileLinkSession?.session_id} {linkBase} />
                      {:else if fileLinkSession}
                        <SharedFiles paths={part.paths} sessionId={fileLinkSession.session_id} />
                      {/if}
                    {/each}
                  {/if}
                </div>
                {#if isClamped}
                  <Button
                    variant="link"
                    size="sm"
                    class="mt-0.5 h-7 gap-1 px-0 text-[13px] font-medium touch-target:h-9 touch-target:text-sm"
                    aria-expanded={expanded}
                    aria-controls={`bot-context-body-${message.id}`}
                    onclick={() => toggle(message.id, !expanded)}
                  >{#if expanded}<CaretUpIcon />{$t('bots.showLess')}{:else}<CaretDownIcon />{$t('bots.showMore')}{/if}</Button>
                {/if}
              </div>
            </article>
          </li>
        {/each}
      </ol>
      {#if messages.length === 1}<p class="px-4 pt-4 text-center text-[13px] text-muted-foreground">{$t('bots.noRelated')}</p>{/if}
    </div>
  {/if}
  </div>
</Tooltip.Provider>

<style>
  .context-body-clamped {
    max-height: 7em;
    overflow: hidden;
    mask-image: linear-gradient(to bottom, #000 calc(100% - 1em * 1.75), transparent);
  }
  [data-fullscreen] .context-body-clamped { max-height: 7.2em; }
  [data-mobile] .context-body-clamped { max-height: 6.8em; }
  .context-message-flash { animation: context-message-flash 1.8s ease-out; }
  @keyframes context-message-flash {
    0%, 55% { background: color-mix(in srgb, var(--primary) 16%, transparent); }
    100% { background: transparent; }
  }
</style>
