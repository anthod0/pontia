<script lang="ts">
  import MessageResponse from '../../src/lib/components/ai-elements/message/message-response.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreview } from '../../src/components/chat/filePreviewContext'
  let { content }: { content: string } = $props()
  const preview = provideFilePreview()
</script>

<MessageResponse {content} markdown sessionId="session-1" />
{#if $preview}
  {#key $preview}
    <FilePreviewPanel target={$preview} onclose={() => preview.set(null)} />
  {/key}
{/if}
