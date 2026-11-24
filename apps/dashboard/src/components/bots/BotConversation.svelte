<script lang="ts">
  import BotMessageFeed from './BotMessageFeed.svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import { MessageResponse } from '$lib/components/ai-elements/message/index.js'
  import SharedFiles from '../chat/files/SharedFiles.svelte'
  import { fileActionButton, fileActionClass, type FileActionLook } from '../chat/files/fileActionButton'
  import { parseUploadMessage } from '$lib/uploads/message'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon'
  import { botBubbleClass, highlightableClass, userBubbleClass } from './messageBubble'
  import BotMessageActions from './BotMessageActions.svelte'
  import BotQuoteBlock from './BotQuoteBlock.svelte'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { parseQuotedMessage } from '$lib/botQuote'
  import { splitAttachments } from '$lib/attachmentBlocks'
  import type { BotChatController } from '../../features/bots/botChat.svelte'
  let { chat, oncontext, ontodos, bottomInset = 0, mobile = false }: {chat: BotChatController; bottomInset?: number; mobile?: boolean; oncontext:(id:string)=>void; ontodos:()=>void} = $props()
  const timeline = $derived(chat.timeline)
  let feedRoot = $state<HTMLElement>()
  let actions = $state<BotMessageActions>()
  const quotableProps = { 'data-bot-quotable': '', tabindex: 0, role: 'article' } as const

</script>
<!-- A long press on touch selects text, so touch rows always show this button.
     With a mouse it appears while its message is hovered or keyboard-focused, and while its menu is open. -->
{#snippet menuButton(id: string)}
  <Button variant="ghost" size="icon-sm" aria-haspopup="menu" aria-label={$t('bots.messageActions')}
    class="shrink-0 self-end text-muted-foreground [.mobile-shell_&]:size-11 [.mobile-shell_&]:opacity-100 opacity-0 transition-opacity focus-visible:opacity-100 aria-expanded:opacity-100 pointer-coarse:opacity-100 group-hover/entry:opacity-100 group-has-focus-visible/entry:opacity-100"
    onclick={(event: MouseEvent) => actions?.openMenu(id, event)}><DotsThreeIcon weight="bold" /></Button>
{/snippet}

<!-- A file's menu looks like its download action and always shows; image tiles keep their own reveal. -->
{#snippet fileMenuButton(id: string, look: FileActionLook)}
  <Button {...fileActionButton} class={fileActionClass[look]} aria-haspopup="menu" aria-label={$t('bots.messageActions')}
    onclick={(event: MouseEvent) => actions?.openMenu(id, event)}><DotsThreeIcon weight="bold" /></Button>
{/snippet}

    <BotMessageActions root={feedRoot} fileSessionId={timeline.fileLinkSession?.session_id} entry={id => timeline.entries.find(row => row.id === id)} bind:this={actions} onresolve={id => chat.changeTodo(id, 'resolve')} oncontext={id => oncontext(id)} onquote={quote => void chat.addQuote(quote)} />
    <BotMessageFeed bind:this={chat.feed} bind:root={feedRoot} {timeline} {bottomInset} typing={chat.typing} highlighted={chat.highlighted && !chat.highlightedPath && timeline.entries.some(row => row.id === chat.highlighted) ? chat.highlighted : null}>
      {#snippet row(entry)}
        {#if 'content' in entry}
          {@const quoted = entry.kind === 'user_input' ? parseQuotedMessage(entry.content) : { text: entry.content, quote: null }}
          {@const parts = entry.kind === 'user_input' ? { ...parseUploadMessage(quoted.text), quote: quoted.quote } : { ...quoted, files: [] }}
          {@const fileSession = timeline.fileLinkSession?.session_id}
          <div class={['flex gap-1', entry.kind === 'user_input' ? 'justify-end' : 'justify-start']}>
            {#if !mobile && entry.kind === 'user_input'}{@render menuButton(entry.id)}{/if}
            <div class="max-w-[90%] [.mobile-shell_&]:max-w-[86%] min-w-0 space-y-2">
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
                    <SharedFiles paths={part.paths} sessionId={fileSession} highlighted={chat.highlighted === entry.id ? chat.highlightedPath : null}
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
                    <BotQuoteBlock quote={current} speaker={chat.quoteSpeaker(current)} image={chat.quoteImage(current)} missing={chat.missingQuotes.includes(target)} busy={chat.jumping === target} onjump={() => void chat.jumpTo(target, current.kind === 'file' ? current.path : null)} />
                  {/if}
                  <p class="whitespace-pre-wrap [overflow-wrap:anywhere]" data-bot-quote-source>{parts.text}</p>
                </div>
              {/if}
              <div class={['-mt-1 flex items-center gap-2 text-xs text-muted-foreground', entry.kind === 'user_input' ? 'justify-end' : 'justify-start']} data-bot-message-meta>
                {#if entry.status === 'failed'}
                  <span class="flex items-center gap-2 text-destructive">
                    {$t('bots.sendFailed')}
                    <Button variant="ghost" size="xs" disabled={chat.retrying.includes(entry.id)} onclick={() => void chat.retry(entry.id)}>{$t('common.retry')}</Button>
                  </span>
                {/if}
                {#if mobile && entry.todo?.state === 'open'}
                  <button class="min-h-11 min-w-11 text-primary" onclick={ontodos}>{$t('bots.todos')}</button>
                {/if}
                <MessageTime value={entry.created_at} />
                {#if mobile}{@render menuButton(entry.id)}{/if}
              </div>
            </div>
            {#if !mobile && entry.todo?.state === 'open'}
              <button type="button" class="grid size-7 shrink-0 place-items-center self-end rounded-md outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
                aria-label={$t('bots.todos')} title={$t('bots.todos')} onclick={() => ontodos()}>
                <span class="size-2 rounded-full bg-primary"></span>
              </button>
            {/if}
            {#if !mobile && entry.kind === 'bot_reply'}{@render menuButton(entry.id)}{/if}
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
