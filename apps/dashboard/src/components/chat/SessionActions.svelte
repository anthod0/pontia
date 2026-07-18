<script lang="ts">
  import { EllipsisVertical, Inbox, LogOut, Pencil, RotateCw, SquarePen, TerminalSquare } from '@lucide/svelte'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as ButtonGroup from '$lib/components/ui/button-group/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import type { SessionView } from '../../api/types'
  import { isTerminalChatSession } from '$lib/session-chat/sessionChat'

  interface Props {
    session: SessionView
    inboxActionableCount: number
    actionBusy?: boolean
    onOpenInbox: () => void
    onExit: () => void
    onOpenConsole: () => void
    onNewChat: () => void
    onRename: () => void
    onRestart: () => void
  }

  let {
    session,
    inboxActionableCount,
    actionBusy = false,
    onOpenInbox,
    onExit,
    onOpenConsole,
    onNewChat,
    onRename,
    onRestart,
  }: Props = $props()

  let advancedControlsOpen = $state(false)
</script>

<ButtonGroup.Root class="flex" aria-label="Primary session actions" data-chat-primary-actions>
  <Button variant="ghost" size="sm" class="hidden gap-2 sm:inline-flex" onclick={onNewChat}>
    <SquarePen class="size-4" /> <span>New Chat</span>
  </Button>
  <Button data-chat-desktop-inbox variant="ghost" size="sm" class="relative hidden gap-2 sm:inline-flex" aria-label={`Open inbox, ${inboxActionableCount} message${inboxActionableCount === 1 ? '' : 's'}`} onclick={onOpenInbox}>
    <Inbox class="size-4" /> <span>Inbox</span>
    {#if inboxActionableCount > 0}<Badge variant="secondary" class="absolute -right-2 -top-2 h-5 min-w-5 rounded-full px-1.5 text-xs shadow-sm">{inboxActionableCount}</Badge>{/if}
  </Button>
  {#if !isTerminalChatSession(session)}
    <Button class="hidden text-destructive hover:text-destructive sm:inline-flex" variant="ghost" size="sm" disabled={actionBusy} aria-label="Exit session" onclick={onExit}><LogOut class="size-4" /> Exit</Button>
  {/if}
  <DropdownMenu.Root bind:open={advancedControlsOpen}>
    <DropdownMenu.Trigger disabled={actionBusy} aria-label={inboxActionableCount > 0 ? `Advanced session controls, ${inboxActionableCount} inbox message${inboxActionableCount === 1 ? '' : 's'}` : 'Advanced session controls'}>
      {#snippet child({ props })}
        <Button {...props} variant="ghost" size="icon-sm" class="relative">
          <EllipsisVertical class="size-4" />
          {#if inboxActionableCount > 0}<Badge data-chat-mobile-inbox-count variant="secondary" class="absolute -right-2 -top-2 h-5 min-w-5 rounded-full px-1.5 text-xs shadow-sm sm:hidden">{inboxActionableCount}</Badge>{/if}
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content side="top" align="end" class="w-52">
      <DropdownMenu.Item class="sm:hidden" onclick={() => { advancedControlsOpen = false; onNewChat() }}><SquarePen class="size-4" /> New Chat</DropdownMenu.Item>
      <DropdownMenu.Separator class="sm:hidden" />
      <DropdownMenu.Item class="sm:hidden" aria-label={`Open inbox, ${inboxActionableCount} message${inboxActionableCount === 1 ? '' : 's'}`} onclick={() => { advancedControlsOpen = false; onOpenInbox() }}>
        <Inbox class="size-4" /> Inbox
        {#if inboxActionableCount > 0}<Badge variant="secondary" class="ml-auto h-5 min-w-5 rounded-full px-1.5 text-xs">{inboxActionableCount}</Badge>{/if}
      </DropdownMenu.Item>
      <DropdownMenu.Item disabled={actionBusy} onclick={() => { advancedControlsOpen = false; onRename() }}><Pencil class="size-4" /> Rename session</DropdownMenu.Item>
      <DropdownMenu.Item disabled={actionBusy} onclick={() => { advancedControlsOpen = false; onRestart() }}><RotateCw class="size-4" /> Restart session</DropdownMenu.Item>
      <DropdownMenu.Item onclick={() => { advancedControlsOpen = false; onOpenConsole() }}><TerminalSquare class="size-4" /> Session Console</DropdownMenu.Item>
      {#if !isTerminalChatSession(session)}
        <DropdownMenu.Separator class="sm:hidden" />
        <DropdownMenu.Item variant="destructive" class="sm:hidden" disabled={actionBusy} onclick={() => { advancedControlsOpen = false; onExit() }}><LogOut class="size-4" /> Exit session</DropdownMenu.Item>
      {/if}
    </DropdownMenu.Content>
  </DropdownMenu.Root>
</ButtonGroup.Root>
