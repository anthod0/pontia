<script lang="ts">
  import { statusLabel } from '$lib/i18n/status';
  import { t, currentLocale } from '$lib/i18n';
  import { useDeviceRuntime } from '../../../device/context';
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { navigate } from '$lib/navigation'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { clientDisplayName } from '$lib/clientName'
  import { sessionStateDotClass } from '$lib/sessionState'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { createSessionList } from '../../../features/sessions/sessionList.svelte'
  import type { SessionView } from '../../../api/types'
  const device = useDeviceRuntime();

  let selectedTab = $state('all')
  const sessions = createSessionList(device)
  const overviewQuery = sessions.query
  const overview = $derived(sessions.overview)
  const activeIds = $derived(new Set(overview.active.map((session) => session.session_id)))
  const listSessions = $derived(overview.list.filter((session) => !activeIds.has(session.session_id)))
  const selectedCount = $derived(
    selectedTab === 'archived'
      ? overview.archived.length
      : selectedTab === 'pinned'
        ? overview.pinned.length
        : overview.active.length + listSessions.length,
  )

  function openSession(sessionId: string): void {
    navigate(`/chat/${sessionId}`)
  }
</script>

{#snippet sessionList(items: SessionView[], testId: string)}
  <div class="divide-y" data-testid={testId}>
    {#each items as session (session.session_id)}
      {@const acpAgentId = (session.acp as { agent_id?: string } | undefined)?.agent_id}
      <button type="button" class="w-full rounded-item px-1 py-3 text-left transition hover:bg-muted/50 sm:px-2" onclick={() => openSession(session.session_id)}>
        <div class="flex min-w-0 items-start gap-3">
          <ClientIcon clientType={session.client_type} {acpAgentId} class="mt-0.5 size-8" />
          <div class="flex min-w-0 flex-1 items-start justify-between gap-3">
            <div class="min-w-0 space-y-1">
              <div class="truncate font-medium">{sessionChatTitle(session, $t)}</div>
              <div class="truncate text-sm text-muted-foreground">{clientDisplayName(session.client_type)} {$t('chat.updatedPrefix')} {new Date(session.updated_at).toLocaleString($currentLocale)}</div>
            </div>
            {#if session.state !== 'exited'}
              <Badge variant="secondary" class="shrink-0 gap-1.5">
                <span class={`size-2 rounded-full ${sessionStateDotClass(session.state)}`}></span>
                {statusLabel(session.state, $t)}
              </Badge>
            {/if}
          </div>
        </div>
      </button>
    {/each}
  </div>
{/snippet}

{#snippet emptyState(title: string, description: string)}
  <Card.Root>
    <Card.Content class="py-8">
      <Empty.Root>
        <Empty.Header>
          <Empty.Title>{title}</Empty.Title>
          <Empty.Description>{description}</Empty.Description>
        </Empty.Header>
      </Empty.Root>
    </Card.Content>
  </Card.Root>
{/snippet}

<section class="mx-auto flex w-full max-w-4xl flex-col gap-6" aria-labelledby="sessions-page-title">
  <div class="flex items-center justify-between gap-3">
    <div class="space-y-2">
      <h1 id="sessions-page-title" class="text-3xl font-semibold tracking-tight">{$t('sessions.title')}</h1>
      <p class="text-sm text-muted-foreground">{$t('sessions.description')}</p>
    </div>
    <Badge variant="secondary">{selectedCount}</Badge>
  </div>

  {#if overviewQuery.error}
    <Alert.Root variant="destructive">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>{$t('sessions.loadFailed')}</Alert.Title>
      <Alert.Description>{overviewQuery.error.message}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if overviewQuery.isPending}
    <Card.Root>
      <Card.Content class="py-6 text-sm text-muted-foreground" role="status">{$t('sessions.loading')}</Card.Content>
    </Card.Root>
  {:else}
    <Tabs.Root bind:value={selectedTab}>
      <Tabs.List variant="line" aria-label={$t('sessions.views')}>
        <Tabs.Trigger value="all">{$t('common.all')}</Tabs.Trigger>
        <Tabs.Trigger value="archived">{$t('common.archived')}</Tabs.Trigger>
        <Tabs.Trigger value="pinned">{$t('common.pinned')}</Tabs.Trigger>
      </Tabs.List>

      <Tabs.Content value="all" class="space-y-6 pt-4">
        {#if overview.active.length || listSessions.length}
          {#if overview.active.length}
            <section class="space-y-2" aria-labelledby="active-sessions-title">
              <h2 id="active-sessions-title" class="text-sm font-semibold">{$t('common.active')}</h2>
              {@render sessionList(overview.active, 'active-session-list')}
            </section>
          {/if}

          <section class="space-y-2" aria-labelledby="session-list-title">
            <h2 id="session-list-title" class="text-sm font-semibold">{$t('sessions.all')}</h2>
            {#if listSessions.length}
              {@render sessionList(listSessions, 'all-session-list')}
            {:else}
              <p class="py-4 text-sm text-muted-foreground">{$t('sessions.noOther')}</p>
            {/if}
            {#if overview.nextCursor}
              <div class="flex justify-center pt-2">
                <Button variant="outline" disabled={overviewQuery.isFetchingNextPage} onclick={() => void overviewQuery.fetchNextPage()}>
                  {overviewQuery.isFetchingNextPage ? $t('common.loading') : $t('common.loadMore')}
                </Button>
              </div>
            {/if}
          </section>
        {:else if !overviewQuery.error}
          {@render emptyState($t('sessions.empty'), $t('sessions.startHint'))}
        {/if}
      </Tabs.Content>

      <Tabs.Content value="archived" class="pt-4">
        {#if overview.archived.length}
          {@render sessionList(overview.archived, 'archived-session-list')}
        {:else}
          {@render emptyState($t('sessions.noArchived'), $t('sessions.archivedHint'))}
        {/if}
      </Tabs.Content>

      <Tabs.Content value="pinned" class="pt-4">
        {#if overview.pinned.length}
          {@render sessionList(overview.pinned, 'pinned-session-list')}
        {:else}
          {@render emptyState($t('sessions.noPinned'), $t('sessions.pinnedHint'))}
        {/if}
      </Tabs.Content>
    </Tabs.Root>
  {/if}
</section>
