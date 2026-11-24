<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import type { NewChatController } from '../../../features/chat/newChat.svelte'
  import NewChatLayout from '../../../components/chat/NewChatLayout.svelte'
  import NewChatPanel from '../../../components/chat/NewChatPanel.svelte'
  import NewChatComposerPlaceholder from '../../../components/chat/NewChatComposerPlaceholder.svelte'
  let { chat, deviceSelector }: { chat: NewChatController; deviceSelector?: Snippet<[boolean]> } = $props()
  const chatDraft = $derived(chat.chatDraft)
</script>

<NewChatLayout
  status={chat.loading
    ? { mood: 'loading', title: $t('workspaces.loading'), description: $t('common.fewSeconds') }
    : undefined}
>
  {#if chat.loading}
    <NewChatComposerPlaceholder {deviceSelector} />
  {:else}
    <NewChatPanel
      bind:prompt={$chatDraft}
      bind:workspaceId={chat.workspaceId}
      bind:reasoningEffort={chat.reasoningEffort}
      bind:model={chat.model}
      bind:creationOptionId={chat.creationOptionId}
      creating={chat.creating}
      {deviceSelector}
      canCreate={chat.canCreate}
      autofocus={chat.autofocus}
      workspaces={chat.workspaces}
      workspacesLoading={chat.workspacesLoading}
      selectedWorkspace={chat.selectedWorkspace}
      creationOptions={chat.creationOptions}
      placement="bottom"
      onStartChat={(text) => chat.startChat(text)}
    />
  {/if}
</NewChatLayout>
