<script lang="ts">
  import { onDestroy, type Snippet } from 'svelte'
  import * as ContextMenu from '$lib/components/ui/context-menu'
  import { t, currentLocale } from '$lib/i18n'
  import SessionStateDot from './SessionStateDot.svelte'
  import { navigate } from '$lib/navigation'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { clientSessionLabel } from '../../../clients'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import {formatRelativeTime} from '$lib/format'
  import PushPinIcon from 'phosphor-svelte/lib/PushPinIcon'
  import type { SessionView } from '../../../api/types'
  let {
    session,
    open,
    reveal,
    menu,
    pin,
    archive,
    busy,
    contextMenu,
  }: {
    session: SessionView
    open: boolean
    reveal: (open: boolean) => void
    menu: () => void
    pin: () => void
    archive: () => void
    busy: boolean
    contextMenu: Snippet
  } = $props()
  let start: PointerEvent | null = null,
    timer: ReturnType<typeof setTimeout>,
    suppress = false
  let drag = $state<number | null>(null)
  const acpAgentId = $derived((session.acp as { agent_id?: string } | undefined)?.agent_id)
  const width = 152
  function cancel() {
    clearTimeout(timer)
    start = null
    drag = null
  }
  onDestroy(cancel)
  function down(e: PointerEvent) {
    if (!e.isPrimary || e.button !== 0) return
    cancel()
    start = e
    suppress = false
    timer = setTimeout(() => {
      suppress = true
      cancel()
      navigator.vibrate?.(15)
      menu()
    }, 500)
  }
  function move(e: PointerEvent) {
    if (!start || e.pointerId !== start.pointerId) return
    const dx = e.clientX - start.clientX,
      dy = e.clientY - start.clientY
    if (drag === null && Math.max(Math.abs(dx), Math.abs(dy)) < 8) return
    clearTimeout(timer)
    suppress = true
    if (drag === null && Math.abs(dy) > Math.abs(dx)) {
      cancel()
      return
    }
    drag = Math.min(0, Math.max(-width, (open ? -width : 0) + dx))
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  }
  function end() {
    if (drag !== null) reveal(drag < -width / 3)
    cancel()
  }
</script>

<div class="relative overflow-hidden rounded-item">
  <div aria-hidden={!open} inert={!open} class="absolute inset-y-0 right-0 flex w-[152px]">
    {#if !session.archived_at}<button class="flex-1 bg-selected text-primary" disabled={busy} onclick={pin}
        >{$t(session.pinned_at ? 'sessions.unpin' : 'sessions.pin')}</button
      >{/if}
    <button class="flex-1 bg-muted" disabled={busy} onclick={archive}
      >{$t(session.archived_at ? 'common.unarchive' : 'common.archive')}</button
    >
  </div>
  <ContextMenu.Root>
    <ContextMenu.Trigger>
      {#snippet child({ props })}
        <button
          {...props}
          class="mobile-row relative touch-pan-y select-none bg-background transition-transform"
          style:transition-duration={drag !== null ? '0ms' : undefined}
          style:transform={'translateX(' + (drag ?? (open ? -width : 0)) + 'px)'}
          onpointerdown={down}
          onpointermove={move}
          onpointerleave={() => {
            if (drag === null) cancel()
          }}
          onpointerup={end}
          onpointercancel={cancel}
          onclick={() => {
            if (suppress) {
              suppress = false
              return
            }
            if (open) reveal(false)
            else void navigate('/chat/' + encodeURIComponent(session.session_id))
          }}
        >
          <ClientIcon clientType={session.client_type} {acpAgentId} class="size-8 shrink-0" />
          <span class="min-w-0 flex-1">
            <span class="flex items-center gap-1.5"
              >{#if session.pinned_at}<PushPinIcon class="size-4" />{/if}<strong class="flex-1 truncate"
                >{sessionChatTitle(session, $t)}</strong
              >
              <SessionStateDot state={session.state} />
            </span>
            <small class="flex justify-between text-muted-foreground"
              >{clientSessionLabel(session)}<time datetime={session.updated_at}>{formatRelativeTime(session.updated_at,$currentLocale)}</time></small
            >
          </span>
        </button>
      {/snippet}
    </ContextMenu.Trigger>
    <ContextMenu.Content aria-label={sessionChatTitle(session, $t)} class="min-w-44">
      {@render contextMenu()}
    </ContextMenu.Content>
  </ContextMenu.Root>
</div>
