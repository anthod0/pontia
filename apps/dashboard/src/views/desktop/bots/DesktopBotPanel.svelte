<script lang="ts">
  import SidePanel from '$lib/components/ui/overlay/SidePanel.svelte'
  import { createQuery } from '@tanstack/svelte-query'
  import { onMount, tick, type Snippet } from 'svelte'
  import ListChecksIcon from 'phosphor-svelte/lib/ListChecksIcon'
  import ArrowBendUpLeftIcon from 'phosphor-svelte/lib/ArrowBendUpLeftIcon'
  import EyeSlashIcon from 'phosphor-svelte/lib/EyeSlashIcon'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { useTopBarAction } from '../../../components/layout/topBarAction'
  import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import MagnifyingGlassIcon from 'phosphor-svelte/lib/MagnifyingGlassIcon'
  import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { t } from '$lib/i18n'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { entryQuote, todoSummary, type BotQuote } from '$lib/botQuote'
  import type { Bot, BotMessage, BotSearchType } from '../../../api/types'
  import { useDeviceRuntime } from '../../../device/context'
  import type { LocalhostPreviewTarget } from '$lib/localhostPreview'
  import type { FilePreviewTarget } from '../../../components/chat/filePreviewContext'
  import LocalhostPreviewPanel from '../../../components/chat/LocalhostPreviewPanel.svelte'
  import FilePreviewPanel from '../../../components/chat/FilePreviewPanel.svelte'
  import FilePreviewResizer from '../../../components/chat/FilePreviewResizer.svelte'
  import TodoViews from '../../../components/bots/TodoViews.svelte'
  import BotAvatar from '../../../components/bots/BotAvatar.svelte'
  import BotMessageSearch from '../../../components/bots/BotMessageSearch.svelte'
  import BotGitTab from '../../../components/bots/BotGitTab.svelte'
  import BotContextList from '../../../components/bots/BotContextList.svelte'
  import RecentFileList from '../../../components/chat/files/RecentFileList.svelte'
  import { createRecentFiles, withRecentFile } from '$lib/botRecentFiles'
  import { botPanelTabs, type PanelTab } from '$lib/botPanelTab'
  import { SidePanelRoute } from '$lib/sidePanelRoute.svelte'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import { promptValueAfterEnter } from '$lib/promptEnterBehavior'
  import { registerShortcut } from '$lib/shortcuts/dispatch'
  import { adjacentValue } from '$lib/shortcuts/sessionNavigation'

  let { bot, count, fileLinkSession, onjump, onquote, onreply, ontodochange }: {
    bot: Bot
    count: number
    /** The Session that reads the Bot's shared files and resolves their relative links. */
    fileLinkSession: { session_id: string; workspace: string | null } | null
    ontodochange: (id: string, open: boolean) => void
    /** Scrolls to a message; `path` names its shared file to highlight. */
    onjump: (id: string, path?: string | null) => Promise<void>
    onquote: (quote: BotQuote) => Promise<void>
    /** Sends `text` quoting `quote`; `retry` resubmits the reply box and returns false when it cannot. */
    onreply: (quote: BotQuote, text: string, retry: () => boolean) => Promise<boolean>
  } = $props()
  const fileSessionId = $derived(fileLinkSession?.session_id ?? null)
  const action = useTopBarAction()
  onMount(() => {
    action?.set(sidebarButton)
    return () => action?.set(null)
  })
  const device = useDeviceRuntime()
  // The Bot chat page's URL says whether the panel is open and on which tab.
  const panel = new SidePanelRoute<PanelTab>(botPanelTabs)
  registerShortcut('switchBotPanelTab', event => {
    const next = adjacentValue([...botPanelTabs], panel.tab, event.shiftKey ? -1 : 1) as PanelTab
    void panel.show(next)
  })
  let preview = $state.raw<LocalhostPreviewTarget | null>(null)
  let file = $state.raw<FilePreviewTarget | null>(null)
  // The panel lives as long as its Bot chat page, which is recreated for another Bot.
  // svelte-ignore state_referenced_locally
  const recentFiles = createRecentFiles(device.id, bot.bot_id)
  let selected = $state<string | null>(null)
  let searchText = $state('')
  let searchFilter = $state<BotSearchType>('all')
  let searchSelected = $state<string | null>(null)
  const context = createQuery(() => ({
    queryKey: ['bot-message-context', bot.bot_id, selected],
    queryFn: ({ signal }) => device.api.getBotMessageContext(bot.bot_id, selected!, signal),
    enabled: panel.open && panel.tab === 'context' && selected !== null,
  }), () => device.queryClient)
  const todos = createQuery(() => ({
    queryKey: ['bot-todos', bot.bot_id],
    queryFn: ({ signal }) => device.api.getBotTodos(bot.bot_id, signal),
    enabled: panel.open && panel.tab === 'todo',
    staleTime: 0,
  }), () => device.queryClient)

  export function openPreview(target: LocalhostPreviewTarget) {
    preview = { ...target }
    void panel.show('preview')
  }

  export function openFile(target: FilePreviewTarget) {
    file = target
    recentFiles.current = withRecentFile(recentFiles.current, { sessionId: target.sessionId, path: target.path })
    void panel.show('files')
  }

  export function viewContext(id: string) {
    selected = id
    void panel.show('context')
  }

  export function viewTodos() {
    void panel.show('todo')
  }

  async function closeOverlay() {
    if (!panel.wide && panel.open) {
      await panel.close()
      await tick()
    }
  }

  async function jump(id: string, path: string | null = null) {
    await closeOverlay()
    await onjump(id, path)
  }

  function jumpToResult(id: string, path: string | null) {
    searchSelected = id
    void jump(id, path)
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
  {#if count > 0}<Badge class={['h-4 min-w-4 rounded-chip px-1 text-10 tabular-nums', corner && 'pointer-events-none absolute -top-1 -right-1']}>{count > 99 ? '99+' : count}</Badge>{/if}
{/snippet}

{#snippet sidebarButton()}
  <Button variant="ghost" size="icon-circle" class={['pointer-events-auto mr-3 relative size-8 p-0 ring-1 ring-foreground/10', panel.open ? 'bg-muted text-heading' : 'bg-popover']}
    aria-label={count > 0 ? $t('bots.sidebarWithTodos', { count }) : $t('nav.sidebar')} title={$t('nav.sidebar')} aria-pressed={panel.open} onclick={() => { void (panel.open ? panel.close() : panel.show()) }}>
    <SidebarSimpleIcon class="size-4 -scale-x-100" />{@render badge(true)}
  </Button>
{/snippet}

{#snippet empty(files: boolean)}
  <Empty.Root class="min-h-0 p-8">
    <Empty.Header>
      <Empty.Media class="size-12 rounded-item bg-muted text-muted-foreground">
        {#if files}<FileTextIcon class="size-6" />{:else}<TreeStructureIcon class="size-6" />{/if}
      </Empty.Media>
      <Empty.Title>{$t(files ? 'files.noneOpen' : 'bots.noSelection')}</Empty.Title>
      {#if !files}<Empty.Description>{$t('bots.noSelectionHint')}</Empty.Description>{/if}
    </Empty.Header>
  </Empty.Root>
{/snippet}

{#snippet todoCard(message: BotMessage)}
  <li class="rounded-item border bg-background p-3">
    <button type="button" class="block w-full rounded-item text-left outline-none focus-visible:ring-2 focus-visible:ring-ring" onclick={() => jump(message.id)}>
      <span class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
        {#if message.kind === 'user_input'}{$t('bots.you')}{:else}<BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} class="size-4" />{bot.name}{/if}
        <MessageTime value={message.created_at} class="px-0" />
      </span>
      <span class="line-clamp-3 text-13 leading-5 [overflow-wrap:anywhere]">{todoSummary(message)}</span>
    </button>
    <div class="mt-2 flex items-center gap-1 touch-target:flex-wrap touch-target:gap-2 touch-target:[&>button]:min-h-11 touch-target:[&>button]:min-w-11">
      <Button variant="ghost" size="xs" onclick={() => viewContext(message.id)}><TreeStructureIcon />{$t('bots.context')}</Button>
      <Button variant="ghost" size="xs" class="ml-auto" onclick={() => ontodochange(message.id, false)}><EyeSlashIcon />{$t('bots.cancelTodo')}</Button>
      <Button variant="ghost" size="xs" onclick={() => quoteTodo(message)}><QuotesIcon />{$t('bots.quote')}</Button>
      <Button id="bot-todo-reply-{message.id}" variant="outline" size="xs" aria-expanded={reply?.message.id === message.id} aria-controls="bot-todo-reply-box-{message.id}" onclick={() => toggleReply(message)}><ArrowBendUpLeftIcon />{$t('bots.reply')}</Button>
    </div>
    {#if reply?.message.id === message.id}
      {@const current = reply}
      <PromptInput.Root id="bot-todo-reply-box-{message.id}" class="mt-2 p-3" aria-label={$t('bots.reply')} onSubmit={() => submitReply(current)}>
        <PromptInput.Textarea {@attach node => node.focus()} bind:value={current.text} placeholder={$t('bots.message')} aria-label={$t('bots.message')} readonly={current.sending} onkeydown={(event: KeyboardEvent) => replyKeydown(event, current)} class="p-0 text-13" />
        <PromptInput.Toolbar class="justify-end gap-1 px-0 pt-2 pb-0">
          <Button variant="ghost" size="xs" disabled={current.sending} onclick={closeReply}>{$t('common.cancel')}</Button>
          <PromptInput.Submit class="h-6 touch-target:min-h-11 gap-1 px-2.5 text-xs" busy={current.sending} disabled={!current.text.trim()} />
        </PromptInput.Toolbar>
      </PromptInput.Root>
    {/if}
  </li>
{/snippet}

{#snippet panelContent(fullscreen: boolean, fullscreenButton: Snippet)}
  {#if !fullscreen}<FilePreviewResizer />{/if}
  <Tabs.Root bind:value={() => panel.tab, next => { panel.tab = next as PanelTab }} class="min-h-0 flex-1 gap-0">
    <header class="flex h-12 shrink-0 items-center justify-between gap-1 border-b px-2">
      <Tabs.List variant="line" aria-label={$t('nav.sidebar')} class="h-full min-w-0 justify-start gap-0 overflow-x-auto pb-1.5">
        <Tabs.Trigger value="files" class="h-full px-2.5"><FileTextIcon />{$t('files.title')}</Tabs.Trigger>
        <Tabs.Trigger value="preview" class="h-full px-2.5"><GlobeIcon />{$t('preview.title')}</Tabs.Trigger>
        <Tabs.Trigger value="context" class="h-full px-2.5"><TreeStructureIcon />{$t('bots.context')}</Tabs.Trigger>
        <Tabs.Trigger value="todo" class="h-full px-2.5"><ListChecksIcon />{$t('bots.todos')}{@render badge()}</Tabs.Trigger>
        <Tabs.Trigger value="search" class="h-full px-2.5"><MagnifyingGlassIcon />{$t('bots.searchTab')}</Tabs.Trigger>
        <Tabs.Trigger value="git" class="h-full px-2.5"><GitBranchIcon />{$t('git.title')}</Tabs.Trigger>
      </Tabs.List>
      <div class="flex shrink-0 items-center gap-1">
        {@render fullscreenButton()}
        <Button variant="ghost" size="icon-circle-sm" class="p-0 max-lg:size-11 pointer-coarse:size-11" aria-label={$t('common.close')} title={$t('common.close')} onclick={() => panel.close()}><CaretRightIcon /></Button>
      </div>
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
      {:else}
        <BotContextList
          {bot}
          {selected}
          messages={context.data?.messages}
          truncated={context.data?.truncated}
          error={context.error}
          {fullscreen}
          {fileLinkSession}
          onretry={() => void context.refetch()}
          onjump={jump}
        />
      {/if}
    </Tabs.Content>
    <Tabs.Content value="search" class="flex min-h-0 flex-col">
      <BotMessageSearch {bot} active={panel.open && panel.tab === 'search'} bind:text={searchText} bind:filter={searchFilter} {fileSessionId} selected={searchSelected} onselect={jumpToResult} />
    </Tabs.Content>
    <Tabs.Content value="git" class="flex min-h-0 flex-col">
      {#if panel.tab === 'git'}<BotGitTab botId={bot.bot_id} />{/if}
    </Tabs.Content>
    <Tabs.Content value="todo" class="flex min-h-0 flex-col">
      <div class="min-h-0 flex-1 overflow-auto overscroll-contain p-3">
        {#if todos.isError}
          <div role="alert" class="text-sm">{todos.error.message} <Button variant="ghost" size="xs" onclick={() => todos.refetch()}>{$t('common.retry')}</Button></div>
        {:else if todos.data}
          {@const data = todos.data}
          <TodoViews count={data.open.length} groups={[{ bot, items: data.recently_resolved }]}
            onjump={(_, item) => jump(item.id)} oncontext={(_, item) => viewContext(item.id)}
            onreopen={(_, item) => ontodochange(item.id, true)}>
            {#snippet pending()}
              {#if data.open.length}
                <ul class="space-y-2">
                  {#each data.open as message (message.id)}
                    {@render todoCard(message)}
                  {/each}
                </ul>
              {:else}
                <Empty.Root class="min-h-0 p-6"><Empty.Header>
                  <Empty.Media class="size-12 rounded-item bg-muted text-muted-foreground"><CheckCircleIcon class="size-6" /></Empty.Media>
                  <Empty.Title>{$t('bots.noTodos')}</Empty.Title>
                </Empty.Header></Empty.Root>
              {/if}
            {/snippet}
          </TodoViews>
        {:else}
          <p role="status" class="text-sm text-muted-foreground">{$t('common.loading')}</p>
        {/if}
      </div>
    </Tabs.Content>
  </Tabs.Root>
{/snippet}

<SidePanel open={panel.open} close={() => panel.close()} data-bot-chat-panel aria-label={$t('nav.sidebar')} children={panelContent} />
