<script lang="ts">
  import SharedFiles from '../../src/components/chat/files/SharedFiles.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreview } from '../../src/components/chat/filePreviewContext'
  let { paths, highlighted = null }: { paths: string[]; highlighted?: string | null } = $props()
  const preview = provideFilePreview()
</script>

<SharedFiles {paths} sessionId="main" {highlighted} />
{#if $preview}
  <FilePreviewPanel target={$preview} onclose={() => preview.set(null)} />
{/if}
