<script lang="ts">
  import MessageResponse from '../../src/lib/components/ai-elements/message/message-response.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreviewNavigator, type FilePreviewTarget } from '../../src/components/chat/filePreviewContext'
  let { content, linkBase = null }: { content: string; linkBase?: string | null } = $props()
  let preview = $state.raw<FilePreviewTarget | null>(null)
  provideFilePreviewNavigator({ open: target => { preview = target } })
</script>

<MessageResponse {content} markdown sessionId="session-1" {linkBase} />
{#if preview}
  {#key preview}
    <aside aria-label="Preview panel"><FilePreviewPanel target={preview} onclose={() => { preview = null }} /></aside>
  {/key}
{/if}
