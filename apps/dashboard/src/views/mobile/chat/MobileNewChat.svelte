<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { useDashboardHost } from '../../../device/hostContext'
  import { useDeviceRuntime } from '../../../device/context'
  import { creationOptionCatalog } from '../../../queries/modelCatalog'
  import type { NewChatController } from '../../../features/chat/newChat.svelte'
  import PontiaMascot from '../../../components/chat/PontiaMascot.svelte'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import MobileWorkspaceSheet from './MobileWorkspaceSheet.svelte'
  import MobileMessageComposer from './MobileMessageComposer.svelte'
  import MobileClientModelSheet from './MobileClientModelSheet.svelte'
  import { workspaceTitle } from '../../../components/chat/sessionMetadata'
  let { chat }: { chat: NewChatController } = $props()
  const host = useDashboardHost(),
    device = useDeviceRuntime()
  const catalog = createQuery(
    () => creationOptionCatalog(device, chat.selectedCreationOption, chat.workspaceId),
    () => device.queryClient,
  )
  const modelName = $derived(
    catalog.data?.models.find((m) => m.id === (chat.model || catalog.data?.default_model))?.name ?? chat.model,
  )
  const prompt = $derived(chat.chatDraft)
  let sheet = $state<'workspace' | 'client' | null>(null)
  const workspace = $derived(
    chat.selectedWorkspace ? workspaceTitle(chat.selectedWorkspace) : $t('workspace.title'),
  )
  const chip = 'mobile-glass flex min-h-11 min-w-0 items-center gap-2 rounded-full px-3'
</script>

<section class="flex h-full min-h-0 flex-col motion-safe:animate-in motion-safe:slide-in-from-bottom-6">
  <div class="grid min-h-0 flex-1 place-content-center gap-2 overflow-auto text-center">
    <PontiaMascot class="mx-auto size-16" />
    <h2 class="font-semibold">
      {chat.loading
        ? $t('workspaces.loading')
        : $t('chat.startInPrefix') +
          ' ' +
          workspace +
          ' ' +
          $t('chat.useClientPrefix') +
          ' ' +
          (chat.selectedCreationOption?.name ?? '')}
    </h2>
    <small class="font-mono">{chat.selectedWorkspace?.display_path ?? ''}</small>
    <small class="text-muted-foreground"
      >{host.activeDevice?.name}{#if modelName}
        · {modelName}{/if}{#if chat.reasoningEffort}
        · {$t('effort.title')} {chat.reasoningEffort}{/if}</small
    >
  </div>
  <div class="mb-2 flex gap-2">
    <button
      class={chip + ' max-w-2/5'}
      disabled={chat.creating || !chat.workspaces.length}
      onclick={() => (sheet = 'workspace')}><FolderIcon class="size-4 shrink-0" /><span class="truncate">{workspace}</span></button
    >
    <button
      class={chip}
      disabled={chat.creating || !chat.creationOptions.length}
      onclick={() => (sheet = 'client')}
      ><ClientIcon
        clientType={chat.selectedCreationOption?.client_type}
        acpAgentId={chat.selectedCreationOption?.metadata.acp_agent_id}
      /><span class="truncate"
        >{chat.selectedCreationOption?.name ?? $t('clients.choose')}{#if modelName}
          · {modelName}{/if}</span
      ></button
    >
  </div>
  <MobileMessageComposer
    autofocus={!!chat.workspaceId}
    bind:value={$prompt}
    workspaceId={chat.workspaceId}
    draftId={'new:' + chat.workspaceId}
    commands={[]}
    busy={chat.creating}
    submitDisabled={!chat.canCreate}
    onSubmit={(text) => chat.startChat(text, true)}
  />
</section>
{#if sheet === 'workspace'}<MobileWorkspaceSheet
    workspaces={chat.workspaces}
    value={chat.workspaceId}
    choose={(id) => (chat.workspaceId = id)}
    close={() => (sheet = null)}
  />
{:else if sheet === 'client'}<MobileClientModelSheet {chat} close={() => (sheet = null)} />{/if}
