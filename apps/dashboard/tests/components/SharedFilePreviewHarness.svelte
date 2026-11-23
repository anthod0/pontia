<script lang="ts">
  import SessionConversation from '../../src/lib/components/session-chat/SessionConversation.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreviewNavigator, type FilePreviewTarget } from '../../src/components/chat/filePreviewContext'
  let { paths }: { paths: string[] } = $props()
  let preview = $state.raw<FilePreviewTarget | null>(null)
  provideFilePreviewNavigator({ open: target => { preview = target } })
  const content = $derived(['```attachments', ...paths, '```'].join('\n'))
</script>

<SessionConversation
  sessionId="session-1"
  messages={[
    { id: 'u1', role: 'user', content: 'share', status: 'sent', turnId: 't1', createdAt: '' },
    { id: 'a1', role: 'assistant', content, status: 'sent', turnId: 't1', createdAt: '' },
  ]}
/>
{#if preview}
  <aside aria-label="Preview panel"><FilePreviewPanel target={preview} onclose={() => { preview = null }} /></aside>
{/if}
