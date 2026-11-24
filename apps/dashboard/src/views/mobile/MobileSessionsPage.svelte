<script lang="ts">
  import ClientIcon from '../../components/chat/ClientIcon.svelte'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { clientDisplayName } from '$lib/clientName'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { statusLabel } from '$lib/i18n/status'
  import { createSessionList } from '../../features/sessions/sessionList.svelte'
  import { useDeviceRuntime } from '../../device/context'
  const sessions = createSessionList(useDeviceRuntime())
</script>

{#each sessions.list as session (session.session_id)}
  <button class="mobile-row" onclick={() => navigate('/chat/' + encodeURIComponent(session.session_id))}>
    <ClientIcon clientType={session.client_type} class="size-6 shrink-0" />
    <span class="min-w-0 flex-1">
      <span class="block truncate font-medium">{sessionChatTitle(session,$t)}</span>
      <small class="text-muted-foreground">{clientDisplayName(session.client_type)} · <MessageTime value={session.updated_at} /></small>
    </span>
    <small class="text-muted-foreground">{statusLabel(session.state,$t)}</small>
  </button>
{:else}
  <p class="py-6 text-muted-foreground" role="status">{sessions.query.error?.message ?? $t(sessions.query.isPending ? 'common.loading' : 'sessions.empty')}</p>
{/each}
{#if sessions.query.hasNextPage}<button class="mobile-row" disabled={sessions.query.isFetchingNextPage} onclick={() => sessions.query.fetchNextPage()}>{$t('common.loadMore')}</button>{/if}
<button class="absolute right-4 bottom-[calc(var(--mobile-tab-h)+var(--mobile-tab-gap)+14px)] flex h-13 items-center gap-2 rounded-2xl bg-primary px-5 text-primary-foreground shadow-md" onclick={() => navigate('/new')}><PlusIcon />{$t('nav.newChat')}</button>
