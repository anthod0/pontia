<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import { compactionDisabledReason } from '$lib/session-chat/compaction'
  import { isTerminalSession } from '$lib/sessionState'
  import type { SessionView } from '../../../api/types'

  type Action = {
    disabled?: boolean
    destructive?: boolean
    select: () => void
  }

  let {
    session,
    fileCount,
    gitSummary,
    openFiles,
    openGit,
    newSession,
    rename,
    pin,
    archive,
    compact,
    exit,
    compacting = false,
    busy = false,
    item,
  }: {
    session: SessionView
    fileCount: number
    gitSummary: string
    openFiles: () => void
    openGit: () => void
    newSession: () => void
    rename: () => void
    pin: () => void
    archive: () => void
    compact: () => void
    exit: () => void
    compacting?: boolean
    busy?: boolean
    item: Snippet<[Action, Snippet]>
  } = $props()
</script>

{#snippet filesContent()}{$t('files.title')}<span class="ml-auto">{fileCount}</span>{/snippet}
{#snippet gitContent()}{$t('git.title')}<span class="ml-auto">{gitSummary}</span>{/snippet}
{#snippet newContent()}{$t('chat.newInWorkspace')}{/snippet}
{#snippet renameContent()}{$t('common.rename')}{/snippet}
{#snippet pinContent()}{$t(session.pinned_at ? 'sessions.unpin' : 'sessions.pin')}{/snippet}
{#snippet archiveContent()}{$t(session.archived_at ? 'common.unarchive' : 'common.archive')}{/snippet}
{#snippet compactContent()}{$t('compaction.action')}{/snippet}
{#snippet exitContent()}{$t('chat.endCurrent')}{/snippet}

{@render item({ select: openFiles }, filesContent)}
{@render item({ select: openGit }, gitContent)}
{@render item({ select: newSession }, newContent)}
{@render item({ select: rename }, renameContent)}
{#if !session.archived_at}
  {@render item({ select: pin, disabled: busy }, pinContent)}
{/if}
{@render item({ select: archive, disabled: busy }, archiveContent)}
{@render item({ select: compact, disabled: !!compactionDisabledReason(session, compacting, $t) }, compactContent)}
{@render item({ select: exit, disabled: isTerminalSession(session), destructive: true }, exitContent)}
