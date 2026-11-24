<script lang="ts">
  import SidePanel from '$lib/components/ui/overlay/SidePanel.svelte'
  import { createQuery } from '@tanstack/svelte-query'
  import { onMount, tick } from 'svelte'
  import ListChecksIcon from 'phosphor-svelte/lib/ListChecksIcon'
  import ArrowBendUpLeftIcon from 'phosphor-svelte/lib/ArrowBendUpLeftIcon'
  import EyeSlashIcon from 'phosphor-svelte/lib/EyeSlashIcon'
  import ArrowUUpLeftIcon from 'phosphor-svelte/lib/ArrowUUpLeftIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { useTopBarAction } from '../../../components/layout/topBarAction'
  import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import MagnifyingGlassIcon from 'phosphor-svelte/lib/MagnifyingGlassIcon'
  import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { t } from '$lib/i18n'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { entryQuote, entryText, parseQuotedMessage, plainText, quoteSummary, type BotQuote } from '$lib/botQuote'
  import type { Bot, BotMessage } from '../../../api/types'
  import { useDeviceRuntime } from '../../../device/context'
  import type { LocalhostPreviewTarget } from '$lib/localhostPreview'
  import type { FilePreviewTarget } from '../../../components/chat/filePreviewContext'
  import LocalhostPreviewPanel from '../../../components/chat/LocalhostPreviewPanel.svelte'
  import FilePreviewPanel from '../../../components/chat/FilePreviewPanel.svelte'
  import FilePreviewResizer from '../../../components/chat/FilePreviewResizer.svelte'
  import BotAvatar from '../../../components/bots/BotAvatar.svelte'
  import BotMessageSearch from '../../../components/bots/BotMessageSearch.svelte'
  import BotGitTab from '../../../components/bots/BotGitTab.svelte'
  import RecentFileList from '../../../components/chat/files/RecentFileList.svelte'
  import { createRecentFiles, withRecentFile } from '$lib/botRecentFiles'
  import { createPanelTab, type PanelTab } from '$lib/botPanelTab'
  import { SidePanelOpen } from '$lib/sidePanelOpen.svelte'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import { promptValueAfterEnter } from '$lib/promptEnterBehavior'

  let { bot, count, onjump, onquote, onreply, ontodochange }: {
    bot: Bot
    count: number
    ontodochange: (id: string, action: 'resolve' | 'reopen') => void
    onjump: (id: string) => Promise<void>
    onquote: (quote: BotQuote) => Promise<void>
    /** Sends `text` quoting `quote`; `retry` resubmits the reply box and returns false when it cannot. */
    onreply: (quote: BotQuote, text: string, retry: () => boolean) => Promise<boolean>
  } = $props()
  const action = useTopBarAction()
  onMount(() => {
    action?.set(sidebarButton)
    return () => action?.set(null)
  })
  const device = useDeviceRuntime()
  // The panel stays open or closed across Bot chats, like its tab.
  const panel = new SidePanelOpen('pontia.botPanelOpen')
  const tab = createPanelTab()
  let preview = $state.raw<LocalhostPreviewTarget | null>(null)
  let file = $state.raw<FilePreviewTarget | null>(null)
  // The panel lives as long as its Bot chat page, which is recreated for another Bot.
  // svelte-ignore state_referenced_locally
  const recentFiles = createRecentFiles(device.id, bot.bot_id)
  let selected = $state<string | null>(null)
  let searchText = $state('')
  let searchSelected = $state<string | null>(null)
  const context = createQuery(() => ({
    queryKey: ['bot-message-context', bot.bot_id, selected],
    queryFn: ({ signal }) => device.api.getBotMessageContext(bot.bot_id, selected!, signal),
    enabled: panel.current && tab.current === 'context' && selected !== null,
  }), () => device.queryClient)
  const todos = createQuery(() => ({
    queryKey: ['bot-todos', bot.bot_id],
    queryFn: ({ signal }) => device.api.getBotTodos(bot.bot_id, signal),
    enabled: panel.current && tab.current === 'todo',
    staleTime: 0,
  }), () => device.queryClient)

  export function openPreview(target: LocalhostPreviewTarget) {
    preview = { ...target }
    tab.current = 'preview'
    panel.current = true
  }

  export function openFile(target: FilePreviewTarget) {
    file = target
    recentFiles.current = withRecentFile(recentFiles.current, { sessionId: target.sessionId, path: target.path })
    tab.current = 'files'
    panel.current = true
  }

  export function viewContext(id: string) {
    selected = id
    tab.current = 'context'
    panel.current = true
  }

  export function viewTodos() {
    tab.current = 'todo'
    panel.current = true
  }

  function withoutLinks(message: BotMessage, text: string) {
    return message.kind === 'bot_reply' ? text.replace(/!?\[([^\]]+)\]\([^)]+\)/g, '$1') : text
  }

  function summary(message: BotMessage) {
    const quote = entryQuote(message)
    return quote ? withoutLinks(message, quoteSummary(quote)) : ''
  }

  /** The whole message as plain text, so the Context tab never cuts it short. */
  function fullText(message: BotMessage) {
    return withoutLinks(message, plainText(entryText(message)))
  }

  async function closeOverlay() {
    if (!panel.wide && panel.current) {
      const closed = new Promise<void>(resolve => window.addEventListener('popstate', () => resolve(), { once: true }))
      panel.current = false
      await closed
      await tick()
    }
  }

  async function jump(id: string) {
    await closeOverlay()
    await onjump(id)
  }

  function jumpToResult(id: string) {
    searchSelected = id
    void jump(id)
  }

  async function quoteTodo(message: BotMessage) {
    await closeOverlay()
    await onquote(entryQuote(message)!)
  }

  // One todo at a time has its reply box open; it stays open while sending so a failure keeps the text.
  type Reply = { message: BotMessage; text: string; sending: boolean }
  let reply = $state<Reply | null>(null)

  function toggleReply(message: BotMessage) {
    if (reply?.sending) return
    reply = reply?.message.id === message.id ? null : { message, text: '', sending: false }
  }

  function closeReply() {
    if (!reply || reply.sending) return
    const id = reply.message.id
    reply = null
    document.getElementById(`bot-todo-reply-${id}`)?.focus()
  }

  // Retrying from the error toast resends the reply that failed, and only while its box is still open.
  function submitReply(current: Reply): boolean {
    if (reply !== current || current.sending || !current.text.trim()) return false
    current.sending = true
    void onreply(entryQuote(current.message)!, current.text, () => submitReply(current)).then(sent => {
      current.sending = false
      if (sent) closeReply()
    })
    return true
  }

  function replyKeydown(event: KeyboardEvent, current: Reply) {
    if (event.isComposing || event.keyCode === 229) return
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      closeReply()
      return
    }
    // Alt+Enter sends too, as in the chat input.
    if (event.key !== 'Enter' || event.shiftKey || event.ctrlKey || event.metaKey || current.sending) return
    event.preventDefault()
    const next = event.altKey ? null : promptValueAfterEnter(current.text)
    if (next !== null) current.text = next
    else submitReply(current)
  }
</script>

{#snippet badge(corner = false)}
  {#if count > 0}<Badge class={['h-4 min-w-4 rounded-full px-1 text-[0.625rem] tabular-nums', corner && 'pointer-events-none absolute -top-1 -right-1']}>{count > 99 ? '99+' : count}</Badge>{/if}
{/snippet}

{#snippet sidebarButton()}
  <Button variant="ghost" size="icon" class={['pointer-events-auto mr-3 relative size-8 rounded-full p-0 ring-1 ring-foreground/10', panel.current ? 'bg-muted text-heading' : 'bg-popover']}
    aria-label={count > 0 ? $t('bots.sidebarWithTodos', { count }) : $t('nav.sidebar')} title={$t('nav.sidebar')} aria-pressed={panel.current} onclick={() => { panel.current = !panel.current }}>
    <SidebarSimpleIcon class="size-4 -scale-x-100" />{@render badge(true)}
  </Button>
{/snippet}

{#snippet empty(files: boolean)}
  <Empty.Root class="min-h-0 p-8">
    <Empty.Header>
      <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground">
        {#if files}<FileTextIcon class="size-6" />{:else}<TreeStructureIcon class="size-6" />{/if}
      </Empty.Media>
      <Empty.Title>{$t(files ? 'files.noneOpen' : 'bots.noSelection')}</Empty.Title>
    </Empty.Header>
  </Empty.Root>
{/snippet}

{#snippet todoCard(message: BotMessage)}
  {@const done = message.todo?.state === 'done' ? message.todo : null}
  <li class={['rounded-xl border p-3', done ? 'border-transparent bg-muted/50' : 'bg-background']}>
    <button type="button" class="block w-full rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring" onclick={() => jump(message.id)}>
      <span class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
        <BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} class="size-4" />
        <MessageTime value={message.created_at} class="px-0" />
        {#if done}
          <span class="ml-auto">{done.by === 'user' ? $t('bots.resolvedByYou') : $t('bots.closedByBot', { name: bot.name })} · <MessageTime value={done.at} class="px-0" /></span>
        {/if}
      </span>
      <span class={['text-[13px] leading-5 [overflow-wrap:anywhere]', done ? 'line-clamp-2 text-muted-foreground' : 'line-clamp-3']}>{summary(message)}</span>
    </button>
    <div class={['mt-2 flex items-center gap-1 touch-target:flex-wrap touch-target:gap-2 touch-target:[&>button]:min-h-11 touch-target:[&>button]:min-w-11', done && 'text-muted-foreground']}>
      <Button variant="ghost" size="xs" onclick={() => viewContext(message.id)}><TreeStructureIcon />{$t('bots.context')}</Button>
      {#if !done}
        <Button variant="ghost" size="xs" class="ml-auto" onclick={() => ontodochange(message.id, 'resolve')}><EyeSlashIcon />{$t('bots.ignore')}</Button>
        <Button variant="ghost" size="xs" onclick={() => quoteTodo(message)}><QuotesIcon />{$t('bots.quote')}</Button>
        <Button id="bot-todo-reply-{message.id}" variant="outline" size="xs" aria-expanded={reply?.message.id === message.id} aria-controls="bot-todo-reply-box-{message.id}" onclick={() => toggleReply(message)}><ArrowBendUpLeftIcon />{$t('bots.reply')}</Button>
      {:else if done.by === 'user'}
        <Button variant="ghost" size="xs" class="ml-auto" onclick={() => ontodochange(message.id, 'reopen')}><ArrowUUpLeftIcon />{$t('bots.reopen')}</Button>
      {/if}
    </div>
    {#if reply?.message.id === message.id}
      {@const current = reply}
      <PromptInput.Root id="bot-todo-reply-box-{message.id}" class="mt-2" aria-label={$t('bots.reply')} onSubmit={() => submitReply(current)}>
        <PromptInput.Textarea {@attach node => node.focus()} bind:value={current.text} placeholder={$t('bots.message')} aria-label={$t('bots.message')} readonly={current.sending} onkeydown={(event: KeyboardEvent) => replyKeydown(event, current)} class="text-[13px]" />
        <PromptInput.Toolbar class="justify-end gap-1 px-1 pt-0 pb-1">
          <Button variant="ghost" size="xs" disabled={current.sending} onclick={closeReply}>{$t('common.cancel')}</Button>
          <PromptInput.Submit class="h-6 touch-target:min-h-11 gap-1 px-2.5 text-xs" busy={current.sending} disabled={!current.text.trim()} />
        </PromptInput.Toolbar>
      </PromptInput.Root>
    {/if}
  </li>
{/snippet}

<SidePanel open={panel.current} close={() => panel.current = false} data-bot-chat-panel aria-label={$t('nav.sidebar')}>
  <FilePreviewResizer />
  <Tabs.Root bind:value={() => tab.current, next => { tab.current = next as PanelTab }} class="min-h-0 flex-1 gap-0">
    <header class="flex h-12 shrink-0 items-center justify-between gap-1 border-b px-2">
      <Tabs.List variant="line" aria-label={$t('nav.sidebar')} class="h-full min-w-0 justify-start gap-0 overflow-x-auto pb-1.5">
        <Tabs.Trigger value="files" class="h-full px-2.5"><FileTextIcon />{$t('files.title')}</Tabs.Trigger>
        <Tabs.Trigger value="preview" class="h-full px-2.5"><GlobeIcon />{$t('preview.title')}</Tabs.Trigger>
        <Tabs.Trigger value="context" class="h-full px-2.5"><TreeStructureIcon />{$t('bots.context')}</Tabs.Trigger>
        <Tabs.Trigger value="todo" class="h-full px-2.5"><ListChecksIcon />{$t('bots.todos')}{@render badge()}</Tabs.Trigger>
        <Tabs.Trigger value="search" class="h-full px-2.5"><MagnifyingGlassIcon />{$t('bots.searchTab')}</Tabs.Trigger>
        <Tabs.Trigger value="git" class="h-full px-2.5"><GitBranchIcon />{$t('git.title')}</Tabs.Trigger>
      </Tabs.List>
      <Button variant="ghost" size="icon-sm" class="rounded-full p-0 max-lg:size-11 pointer-coarse:size-11" aria-label={$t('common.close')} title={$t('common.close')} onclick={() => { panel.current = false }}><XIcon /></Button>
    </header>
    <Tabs.Content value="preview" class="flex min-h-0 flex-col">
      <LocalhostPreviewPanel bind:target={preview} access={device.access} />
    </Tabs.Content>
    <Tabs.Content value="files" class="flex min-h-0 flex-col">
      {#if file}
        {#key file}<FilePreviewPanel target={file} onclose={() => { file = null }} />{/key}
      {:else if recentFiles.current.length}
        <RecentFileList files={recentFiles.current} onopen={recent => openFile({ device, ...recent })} onclear={() => { recentFiles.current = [] }} />
      {:else}
        {@render empty(true)}
      {/if}
    </Tabs.Content>
    <Tabs.Content value="context" class="flex min-h-0 flex-col">
      {#if !selected}
        {@render empty(false)}
      {:else if context.isError}
        <div role="alert" class="p-3 text-sm">{context.error.message} <Button variant="ghost" size="xs" onclick={() => context.refetch()}>{$t('common.retry')}</Button></div>
      {:else if context.data}
        <div class="min-h-0 flex-1 overflow-auto overscroll-contain p-3">
          {#if context.data.truncated}<p class="pb-3 text-center text-xs text-muted-foreground">{$t('bots.contextTruncated')}</p>{/if}
          <ol class="relative space-y-2 before:absolute before:inset-y-4 before:left-[18px] before:w-px before:bg-border">
            {#each context.data.messages as message (message.id)}
              {@const user = message.kind === 'user_input'}
              <li class="relative">
                <button type="button" aria-current={message.id === selected ? 'true' : undefined}
                  class={['flex w-full gap-2.5 rounded-xl p-2 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring', message.id === selected && 'bg-muted ring-2 ring-primary/50']}
                  onclick={() => jump(message.id)}>
                  <span class="relative grid size-5 shrink-0 place-items-center rounded-full bg-surface ring-4 ring-surface">
                    {#if user}<span aria-hidden="true" class="grid size-5 place-items-center rounded-full bg-border text-[10px] font-semibold text-heading">{$t('bots.you').slice(0, 1)}</span>
                    {:else}<BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} class="size-5" />{/if}
                  </span>
                  <span class="min-w-0 flex-1">
                    <span class="flex flex-wrap items-center gap-x-1.5 text-xs text-muted-foreground">
                      <span class="font-medium text-heading [overflow-wrap:anywhere]">{user ? $t('bots.you') : bot.name}</span>
                      <MessageTime value={message.created_at} class="px-0 whitespace-nowrap" />
                      {#if message.todo?.state === 'open'}<span class="ml-auto rounded bg-primary/10 px-1.5 text-[11px] font-medium whitespace-nowrap text-primary">{$t('bots.todos')}</span>
                      {:else if message.todo?.state === 'done'}<span class="ml-auto flex items-center gap-0.5 whitespace-nowrap"><CheckCircleIcon class="size-3" />{$t('bots.todoDone')}</span>{/if}
                      {#if user && parseQuotedMessage(message.content).quote}<span class="flex items-center gap-0.5 whitespace-nowrap"><QuotesIcon class="size-3" />{$t('bots.quoted')}</span>{/if}
                    </span>
                    <span class="block text-[13px] leading-5 whitespace-pre-line [overflow-wrap:anywhere]">{fullText(message)}</span>
                    {#if message.status === 'failed'}<span class="text-xs text-destructive">{$t('bots.sendFailed')}</span>{/if}
                  </span>
                </button>
              </li>
            {/each}
          </ol>
          {#if context.data.messages.length === 1}<p class="pt-4 text-center text-xs text-muted-foreground">{$t('bots.noRelated')}</p>{/if}
        </div>
      {:else}
        <p role="status" class="p-3 text-sm text-muted-foreground">{$t('common.loading')}</p>
      {/if}
    </Tabs.Content>
    <Tabs.Content value="search" class="flex min-h-0 flex-col">
      <BotMessageSearch {bot} active={panel.current && tab.current === 'search'} bind:text={searchText} selected={searchSelected} onselect={jumpToResult} />
    </Tabs.Content>
    <Tabs.Content value="git" class="flex min-h-0 flex-col">
      {#if tab.current === 'git'}<BotGitTab botId={bot.bot_id} />{/if}
    </Tabs.Content>
    <Tabs.Content value="todo" class="flex min-h-0 flex-col">
      <div class="min-h-0 flex-1 overflow-auto overscroll-contain p-3">
        {#if todos.isError}
          <div role="alert" class="text-sm">{todos.error.message} <Button variant="ghost" size="xs" onclick={() => todos.refetch()}>{$t('common.retry')}</Button></div>
        {:else if todos.data}
          {#if todos.data.open.length}
            <ul class="space-y-2">
              {#each todos.data.open as message (message.id)}
                {@render todoCard(message)}
              {/each}
            </ul>
          {:else}
            <Empty.Root class="min-h-0 p-6"><Empty.Header>
              <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><CheckCircleIcon class="size-6" /></Empty.Media>
              <Empty.Title>{$t('bots.noTodos')}</Empty.Title>
            </Empty.Header></Empty.Root>
          {/if}
          {#if todos.data.recently_resolved.length}
            <Collapsible.Root class="mt-4">
              <Collapsible.Trigger class="group flex items-center gap-1 rounded-sm px-1 text-xs font-medium text-muted-foreground hover:text-foreground focus-visible:outline-ring">
                <CaretRightIcon class="size-3 transition-transform group-aria-expanded:rotate-90" />{$t('bots.recentlyResolved')} {todos.data.recently_resolved.length}
              </Collapsible.Trigger>
              <Collapsible.Content>
                <ul class="mt-2 space-y-2">
                  {#each todos.data.recently_resolved as message (message.id)}{@render todoCard(message)}{/each}
                </ul>
              </Collapsible.Content>
            </Collapsible.Root>
          {/if}
        {:else}
          <p role="status" class="text-sm text-muted-foreground">{$t('common.loading')}</p>
        {/if}
      </div>
    </Tabs.Content>
  </Tabs.Root>
</SidePanel>
