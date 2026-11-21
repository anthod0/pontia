<script lang="ts">
  import SharedFiles from '../../src/components/chat/files/SharedFiles.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreview } from '../../src/components/chat/filePreviewContext'
  import type { SharedFile } from '../../src/api/types'
  let { files, highlighted = null }: { files: SharedFile[]; highlighted?: string | null } = $props()
  const preview = provideFilePreview()
</script>

<SharedFiles {files} sessionId="main" {highlighted} />
{#if $preview}
  <FilePreviewPanel target={$preview} onclose={() => preview.set(null)} />
{/if}
