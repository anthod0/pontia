<script lang="ts">
  import { useMobileHeader } from '../../../shell/mobile/header'
  import { page } from '$app/state'
  import { onMount, untrack } from 'svelte'
  import { t } from '$lib/i18n'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { chromeMetrics } from '$lib/mobile/chromeMetrics'
  import { createRecentFiles, withRecentFile } from '$lib/botRecentFiles'
  import { useDeviceRuntime } from '../../../device/context'
  import { provideFilePreviewNavigator, type FilePreviewTarget } from '../../../components/chat/filePreviewContext'
  import { isImagePath } from '$lib/downloads/localFilePath'
  import FilePreviewPanel from '../../../components/chat/FilePreviewPanel.svelte'
  import BotConversation from '../../../components/bots/BotConversation.svelte'
  import BotChatSkeleton from '../../../components/bots/BotChatSkeleton.svelte'
  import BotAvatar from '../../../components/bots/BotAvatar.svelte'
  import BotQuoteBlock from '../../../components/bots/BotQuoteBlock.svelte'
  import type { BotChatController } from '../../../features/bots/botChat.svelte'
  import MobileMessageComposer from '../chat/MobileMessageComposer.svelte'
  import MobileBotMenuSheet from './MobileBotMenuSheet.svelte'
  import MobileBotTodoSheet from './MobileBotTodoSheet.svelte'
  import MobileBotContextSheet from './MobileBotContextSheet.svelte'
  import MobileBotFilesGitScreen from './MobileBotFilesGitScreen.svelte'
  import MobileBotSearchScreen from './MobileBotSearchScreen.svelte'
  import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon'
  let { chat }: { chat: BotChatController } = $props()
  const timeline = $derived(chat.timeline)
  const device = useDeviceRuntime()
  const stack = useMobileStack()!
  const { title: heading, action } = useMobileHeader()
  const recent = createRecentFiles(
    device.id,
    untrack(() => chat.botId),
  )
  let file = $state.raw<FilePreviewTarget>()
  let tab = $state<'files' | 'git'>('files')
  let dockHeight = $state(0)
  let menu = $state(false)
  let todos = $state(false)
  let context = $state(false)
  let contextId = $state('')
  function jump(id: string) {
    if (stack.layers.length) stack.back(() => jump(id))
    else void chat.jumpTo(id)
  }
  function viewContext(id: string) {
    contextId = id
    context = true
  }
  function openFile(target: FilePreviewTarget) {
    file = target
    recent.current = withRecentFile(recent.current, { sessionId: target.sessionId, path: target.path })
    stack.open({ content: preview, title: target.path.split('/').at(-1) })
  }
  provideFilePreviewNavigator({ open: openFile, allSharedFiles: true })
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
    action.set(menuButton)
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

{#snippet title()}
  {#if timeline.bot}<button
      class="mobile-glass flex min-h-11 min-w-0 items-center gap-2 rounded-full px-3"
      onclick={() => (menu = true)}
    >
      <BotAvatar class="size-6" shape={timeline.bot.avatar_shape} color={timeline.bot.avatar_color} />
      <strong class="truncate">{timeline.bot.name}</strong>
    </button>{/if}
{/snippet}
{#snippet menuButton()}<button
    class="mobile-icon mobile-glass"
    aria-label={$t('chat.actions')}
    onclick={() => (menu = true)}><DotsThreeIcon /></button
  >{/snippet}
{#snippet preview()}
  {#if file}
    {#if isImagePath(file.path)}
      {#await device.previews.imageUrl(file.sessionId, file.path)}
        <p role="status">{$t('files.loadingPreview')}</p>
      {:then url}
        {#if url}<img class="mx-auto max-h-full max-w-full object-contain" src={url} alt={file.path} />{:else}<p>
            {$t('files.previewNotImage')}
          </p>{/if}
      {:catch failure}<p role="alert">{failure.message}</p>{/await}
    {:else}<FilePreviewPanel target={file} onclose={() => stack.back()} />{/if}
  {/if}
{/snippet}
{#snippet files()}<MobileBotFilesGitScreen
    bind:tab
    botId={chat.botId}
    {recent}
    onopen={(item) => openFile({ device, ...item })}
  />{/snippet}
{#snippet search()}{#if timeline.bot}<MobileBotSearchScreen bot={timeline.bot} onselect={jump} />{/if}{/snippet}
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
      class="bot-chat-dock pointer-events-none absolute inset-x-0 bottom-0 z-10 [&>div:not(:first-child)]:pointer-events-auto"
    >
      <div class="pointer-events-none flex items-center justify-between gap-1">
        {#if timeline.status.kind !== 'idle'}
          <p class="flex min-w-0 items-center gap-2 px-3 py-1 text-sm leading-5 text-muted-foreground" role="status">
            <BotAvatar class="size-5 shrink-0" shape={timeline.bot.avatar_shape} color={timeline.bot.avatar_color} working />
            <span class="shimmer truncate" style:--_highlight="var(--foreground)">{timeline.status.kind === 'working' ? timeline.status.title : $t('bots.processing')}</span>
          </p>
        {/if}
        {#if timeline.openTodoCount}<button
            class="mobile-glass pointer-events-auto ml-auto min-h-11 shrink-0 rounded-full px-4"
            onclick={() => (todos = true)}>{$t('bots.todos')} · {timeline.openTodoCount}</button
          >{/if}
      </div>
      {#if chat.quote}<BotQuoteBlock
          quote={chat.quote}
          speaker={chat.quoteSpeaker(chat.quote)}
          image={chat.quoteImage(chat.quote)}
          onremove={() => chat.setQuote(null)}
        />{/if}
      <MobileMessageComposer
        bind:this={chat.composer}
        bind:value={chat.draft}
        draftId={'bot:' + chat.botId}
        busy={chat.reloading}
        commands={[{ name: '/reload', description: $t('bots.reload'), run: chat.reload }]}
        submitDisabled={!!chat.quote && !chat.draft.trim()}
        onSubmit={chat.send}
      />
    </div>
    <MobileBotMenuSheet bind:open={menu} bot={timeline.bot} onselect={select} onsaved={() => timeline.refresh(true)} />
    <MobileBotTodoSheet bind:open={todos} {chat} oncontext={viewContext} onjump={jump} />
    <MobileBotContextSheet bind:open={context} bot={timeline.bot} id={contextId} onjump={jump} />
  {:else if !timeline.refreshError}<BotChatSkeleton />{/if}
</section>
