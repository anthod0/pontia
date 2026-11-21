<script lang="ts">
  import FileBubble from '../../src/components/chat/FileBubble.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreview } from '../../src/components/chat/filePreviewContext'
  import type { SharedFile } from '../../src/api/types'
  let { file }: { file: SharedFile } = $props()
  const preview = provideFilePreview()
</script>

<FileBubble {file} sessionId="main" />
{#if $preview}
  <FilePreviewPanel target={$preview} onclose={() => preview.set(null)} />
{/if}
