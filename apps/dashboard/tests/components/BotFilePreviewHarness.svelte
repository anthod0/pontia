<script lang="ts">
  import BotFileBubble from '../../src/components/bots/BotFileBubble.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreview } from '../../src/components/chat/filePreviewContext'
  import type { SharedFile } from '../../src/api/types'
  let { file }: { file: SharedFile } = $props()
  const preview = provideFilePreview()
</script>

<BotFileBubble {file} sessionId="main" />
{#if $preview}
  <FilePreviewPanel target={$preview} onclose={() => preview.set(null)} />
{/if}
