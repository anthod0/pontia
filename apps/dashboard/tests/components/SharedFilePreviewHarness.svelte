<script lang="ts">
  import SessionConversation from '../../src/lib/components/session-chat/SessionConversation.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreview } from '../../src/components/chat/filePreviewContext'
  let { paths }: { paths: string[] } = $props()
  const preview = provideFilePreview()
  const content = $derived(['```attachments', ...paths, '```'].join('\n'))
</script>

<SessionConversation
  sessionId="session-1"
  messages={[
    { id: 'u1', role: 'user', content: 'share', status: 'sent', turnId: 't1', createdAt: '' },
    { id: 'a1', role: 'assistant', content, status: 'sent', turnId: 't1', createdAt: '' },
  ]}
/>
{#if $preview}
  <FilePreviewPanel target={$preview} onclose={() => preview.set(null)} />
{/if}
