<script lang="ts">
  import SessionConversation from '../../src/lib/components/session-chat/SessionConversation.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreview } from '../../src/components/chat/filePreviewContext'
  import type { SharedFile } from '../../src/api/types'
  let { sharedFiles }: { sharedFiles: SharedFile[] } = $props()
  const preview = provideFilePreview()
</script>

<SessionConversation
  sessionId="session-1"
  messages={[{ id: 'u1', role: 'user', content: 'share', status: 'complete', turnId: 't1' }]}
  {sharedFiles}
/>
{#if $preview}
  <FilePreviewPanel target={$preview} onclose={() => preview.set(null)} />
{/if}
