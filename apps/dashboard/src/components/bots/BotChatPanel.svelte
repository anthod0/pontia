<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { tick } from 'svelte'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { currentLocale, t } from '$lib/i18n'
  import { entryQuote, parseQuotedMessage, quoteSummary } from '$lib/botQuote'
  import type { Bot, BotMessage } from '../../api/types'
  import { useDeviceRuntime } from '../../device/context'
  import type { FilePreviewTarget } from '../chat/filePreviewContext'
  import FilePreviewPanel from '../chat/FilePreviewPanel.svelte'
  import FilePreviewResizer from '../chat/FilePreviewResizer.svelte'
  import BotAvatar from './BotAvatar.svelte'

  let { bot, onjump }: { bot: Bot; onjump: (id: string) => Promise<void> } = $props()
  const device = useDeviceRuntime()
  let open = $state(false)
  let tab = $state('files')
  let file = $state.raw<FilePreviewTarget | null>(null)
  let selected = $state<string | null>(null)
  const context = createQuery(() => ({
    queryKey: ['bot-message-context', bot.bot_id, selected],
    queryFn: ({ signal }) => device.api.getBotMessageContext(bot.bot_id, selected!, signal),
    enabled: open && tab === 'context' && selected !== null,
  }), () => device.queryClient)
  const clock = $derived(new Intl.DateTimeFormat($currentLocale, { hour: '2-digit', minute: '2-digit' }))

  export function openFile(target: FilePreviewTarget) {
    file = target
    tab = 'files'
    open = true
  }

  export function viewContext(id: string) {
    selected = id
    tab = 'context'
    open = true
  }

  export function refreshContext() {
    if (open && tab === 'context' && selected) void context.refetch()
  }

  function summary(message: BotMessage) {
    const quote = entryQuote(message)
    if (!quote) return ''
    const text = quoteSummary(quote)
    return message.kind === 'bot_reply' ? text.replace(/!?\[([^\]]+)\]\([^)]+\)/g, '$1') : text
  }

  async function jump(id: string) {
    if (!window.matchMedia('(min-width: 1024px)').matches) {
      open = false
      await tick()
    }
    await onjump(id)
  }
</script>

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

{#if open}
  <aside data-bot-chat-panel aria-label={$t('nav.sidebar')} class="fixed inset-y-0 right-0 z-50 flex w-full min-w-0 flex-col border-l bg-surface pt-[env(safe-area-inset-top)] lg:w-[var(--file-preview-width)]">
    <FilePreviewResizer />
    <Tabs.Root bind:value={tab} class="min-h-0 flex-1 gap-0">
      <header class="flex h-12 shrink-0 items-center justify-between gap-1 border-b px-2">
        <Tabs.List variant="line" aria-label={$t('nav.sidebar')} class="h-full gap-0">
          <Tabs.Trigger value="files" class="h-full px-2.5"><FileTextIcon />{$t('bots.files')}</Tabs.Trigger>
          <Tabs.Trigger value="context" class="h-full px-2.5"><TreeStructureIcon />{$t('bots.context')}</Tabs.Trigger>
        </Tabs.List>
        <Button variant="ghost" size="icon-sm" aria-label={$t('common.close')} title={$t('common.close')} onclick={() => { open = false }}><XIcon /></Button>
      </header>
      <Tabs.Content value="files" class="flex min-h-0 flex-col">
        {#if file}
          {#key file}<FilePreviewPanel embedded target={file} onclose={() => { file = null }} />{/key}
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
                      <span class="flex items-center gap-1.5 text-xs text-muted-foreground">
                        <span class="font-medium text-heading">{user ? $t('bots.you') : bot.name}</span>
                        <time datetime={message.created_at}>{clock.format(new Date(message.created_at))}</time>
                        {#if user && parseQuotedMessage(message.content).quote}<span class="flex items-center gap-0.5"><QuotesIcon class="size-3" />{$t('bots.quoted')}</span>{/if}
                      </span>
                      <span class="line-clamp-3 text-[13px] leading-5 [overflow-wrap:anywhere]">{summary(message)}</span>
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
    </Tabs.Root>
  </aside>
{/if}
