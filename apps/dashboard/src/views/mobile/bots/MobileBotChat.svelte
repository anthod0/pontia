<script lang="ts">
  import { useMobileHeader } from '../../../shell/mobile/header'
  import { page } from '$app/state'
  import { onMount, untrack } from 'svelte'
  import { t } from '$lib/i18n'
  import { botWorkStatusLabel } from '$lib/i18n/status'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { chromeMetrics } from '$lib/mobile/chromeMetrics'
  import { createRecentFiles, withRecentFile, type RecentFile } from '$lib/botRecentFiles'
  import { useDeviceRuntime } from '../../../device/context'
  import { provideFilePreviewNavigator, type FilePreviewTarget } from '../../../components/chat/filePreviewContext'
  import { isImagePath } from '$lib/downloads/localFilePath'
  import { attachedFile } from '$lib/attachmentBlocks'
  import SharedImageViewer from '../../../components/chat/SharedImageViewer.svelte'
  import FilePreviewPanel from '../../../components/chat/FilePreviewPanel.svelte'
  import BotConversation from '../../../components/bots/BotConversation.svelte'
  import { useDashboardHost } from '../../../device/hostContext'
  import MobileBotChatSkeleton from './MobileBotChatSkeleton.svelte'
  import MobileBotTitlePlaceholder from './MobileBotTitlePlaceholder.svelte'
  import BotAvatar from '../../../components/bots/BotAvatar.svelte'
  import ProfileHint from '../../../components/bots/ProfileHint.svelte'
  import BotQuoteChip from '../../../components/bots/BotQuoteChip.svelte'
  import type { BotChatController } from '../../../features/bots/botChat.svelte'
  import MobileMessageComposer from '../chat/MobileMessageComposer.svelte'
  import MobileBotMenuSheet from './MobileBotMenuSheet.svelte'
  import MobileBotTodoSheet from './MobileBotTodoSheet.svelte'
  import MobileBotContextSheet from './MobileBotContextSheet.svelte'
  import MobileBotFilesGitScreen from './MobileBotFilesGitScreen.svelte'
  import MobileBotSearchScreen from './MobileBotSearchScreen.svelte'
  import ListChecksIcon from 'phosphor-svelte/lib/ListChecksIcon'
  import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'
  let { chat }: { chat: BotChatController } = $props()
  const timeline = $derived(chat.timeline)
  const device = useDeviceRuntime()
  const host = useDashboardHost()
  // Until a public device is online, the chat shows its saved messages and cannot send.
  const connecting = $derived(host.mode === 'public' && host.activeDevice?.state === 'connecting')
  const stack = useMobileStack()!
  const { title: heading, action } = useMobileHeader()
  const recent = createRecentFiles(
    device.id,
    untrack(() => chat.botId),
  )
  let file = $state.raw<FilePreviewTarget>()
  let image = $state.raw<RecentFile>()
  let imageOpen = $state(false)
  let tab = $state<'files' | 'git'>('files')
  let dockHeight = $state(0)
  let menu = $state(false)
  let todos = $state(false)
  let context = $state(false)
  let contextId = $state('')
  function jump(id: string, path: string | null = null) {
    if (stack.layers.length) stack.back(() => jump(id, path))
    else void chat.jumpTo(id, path)
  }
  function viewContext(id: string) {
    contextId = id
    context = true
  }
  function remember({ sessionId, path }: RecentFile) {
    recent.current = withRecentFile(recent.current, { sessionId, path })
  }
  function openFile(target: FilePreviewTarget) {
    remember(target)
    if (isImagePath(target.path)) {
      image = target
      imageOpen = true
      return
    }
    file = target
    stack.open({ content: preview, title: target.path.split('/').at(-1) })
  }
  provideFilePreviewNavigator({ open: openFile, allSharedFiles: true, rememberImage: remember })
  function select(key: 'todos' | 'search' | 'files' | 'git') {
    if (key === 'todos') {
      todos = true
      return
    }
    if (key === 'search') {
      stack.open({ title: $t('bots.search'), content: search })
      return
    }
    tab = key
    stack.open({ title: $t('files.title') + ' / ' + $t('git.title'), content: files })
  }
  onMount(() => {
    heading.set(title)
    action.set(headerActions)
    return () => {
      heading.set(null)
      action.set(null)
    }
  })
  let target = untrack(() => page.url.searchParams.get('message'))
  $effect(() => {
    if (target && timeline.bot) {
      const id = target
      target = null
      untrack(() => void chat.jumpTo(id))
    }
  })
</script>
{#snippet chip()}
  {#if chat.quote}<BotQuoteChip quote={chat.quote} speaker={chat.quoteSpeaker(chat.quote)} image={chat.quoteImage(chat.quote)} onremove={() => chat.setQuote(null)} />{/if}
{/snippet}

{#snippet title()}
  {#if timeline.bot}<button
      class="mobile-glass flex min-h-11 min-w-0 items-center gap-2 rounded-full px-3"
      onclick={() => (menu = true)}
    >
      <BotAvatar class="size-6" shape={timeline.bot.avatar_shape} color={timeline.bot.avatar_color} />
      <strong class="truncate">{timeline.bot.name}</strong>
    </button>{:else if !timeline.refreshError}<MobileBotTitlePlaceholder />{/if}
{/snippet}
{#snippet headerActions()}
  <div class="flex items-center gap-2">
    {#if timeline.openTodoCount}
      <button
        class="mobile-icon mobile-glass relative"
        aria-label={`${$t('bots.todos')} · ${timeline.openTodoCount}`}
        onclick={() => (todos = true)}
      >
        <ListChecksIcon size={20} />
        <span class="absolute -right-1 -top-1 grid h-[18px] min-w-[18px] place-items-center rounded-full bg-primary-solid px-1 text-[11px] font-semibold tabular-nums text-primary-foreground ring-2 ring-surface">{timeline.openTodoCount}</span>
      </button>
    {/if}
    <button class="mobile-icon mobile-glass" aria-label={$t('chat.actions')} onclick={() => (menu = true)}>
      <DotsThreeIcon />
    </button>
  </div>
{/snippet}
{#if imageOpen && image}
  <SharedImageViewer sessionId={image.sessionId} files={[attachedFile(image.path)]} bind:open={imageOpen} />
{/if}
{#snippet preview()}
  {#if file}
    <div class="flex h-full min-w-0 flex-col">
      <FilePreviewPanel target={file} onclose={() => stack.back()} />
    </div>
  {/if}
{/snippet}
{#snippet files()}<MobileBotFilesGitScreen
    bind:tab
    botId={chat.botId}
    {recent}
    onopen={(item) => openFile({ device, ...item })}
  />{/snippet}
{#snippet search()}{#if timeline.bot}<MobileBotSearchScreen bot={timeline.bot} fileSessionId={timeline.fileLinkSession?.session_id ?? null} onselect={jump} />{/if}{/snippet}
<section class="mobile-bot-chat relative flex h-full min-h-0 flex-col">
  {#if timeline.refreshError}<div role="alert">
      {timeline.refreshError}
      <button class="min-h-11" onclick={() => timeline.refresh(true)}>{$t('common.retry')}</button>
    </div>{/if}
  {#if timeline.bot}
    <BotConversation mobile {chat} bottomInset={dockHeight} oncontext={viewContext} ontodos={() => (todos = true)} />
    <div
      bind:clientHeight={dockHeight}
      use:chromeMetrics={'--mobile-footer-h'}
      class="bot-chat-dock pointer-events-none absolute inset-x-0 bottom-0 z-10 [&>div]:pointer-events-auto"
    >
      {#if connecting}
        <p class="flex min-w-0 items-center gap-2 px-3 py-1 text-sm leading-5 text-muted-foreground" role="status">
          <SpinnerGapIcon class="size-4 shrink-0 animate-spin" aria-hidden="true" />
          <span class="truncate">{$t('bots.connectingReadOnly', { name: host.activeDevice?.name ?? '' })}</span>
        </p>
      {:else if timeline.status.kind !== 'idle'}
        <p class="flex min-w-0 items-center gap-2 px-3 py-1 text-sm leading-5 text-muted-foreground" role="status">
          <BotAvatar class="size-5 shrink-0" shape={timeline.bot.avatar_shape} color={timeline.bot.avatar_color} working />
          <span class="shimmer truncate" style:--_highlight="var(--foreground)">{botWorkStatusLabel(timeline.status, $t)}</span>
        </p>
      {/if}
      <ProfileHint {chat} name={timeline.bot.name} />
      <MobileMessageComposer
        bind:this={chat.composer}
        bind:value={chat.draft}
        draftId={'bot:' + chat.botId}
        busy={chat.reloading}
        commands={[{ name: '/reload', description: $t('bots.reload'), run: chat.reload }]}
        prefix={chat.quote ? chip : undefined}
        onRemovePrefix={() => chat.setQuote(null)}
        submitDisabled={connecting || (!!chat.quote && !chat.draft.trim())}
        onSubmit={chat.send}
      />
    </div>
    <MobileBotMenuSheet bind:open={menu} bot={timeline.bot} onselect={select} onsaved={() => timeline.refresh(true)} />
    <MobileBotTodoSheet bind:open={todos} {chat} oncontext={viewContext} onjump={jump} />
    <MobileBotContextSheet bind:open={context} bot={timeline.bot} id={contextId} fileLinkSession={timeline.fileLinkSession} onjump={jump} />
  {:else if !timeline.refreshError}<MobileBotChatSkeleton />{/if}
</section>
