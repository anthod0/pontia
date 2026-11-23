<script lang="ts">
  import SharedFiles from '../../src/components/chat/files/SharedFiles.svelte'
  import FilePreviewPanel from '../../src/components/chat/FilePreviewPanel.svelte'
  import { provideFilePreviewNavigator, type FilePreviewTarget } from '../../src/components/chat/filePreviewContext'
  let { paths, highlighted = null }: { paths: string[]; highlighted?: string | null } = $props()
  let preview = $state.raw<FilePreviewTarget | null>(null)
  provideFilePreviewNavigator({ open: target => { preview = target } })
</script>

<SharedFiles {paths} sessionId="main" {highlighted} />
{#if preview}
  <aside aria-label="Preview panel"><FilePreviewPanel target={preview} onclose={() => { preview = null }} /></aside>
{/if}
