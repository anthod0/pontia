<script lang="ts">
  import { useSessionPanel } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import type { FilePreviewTarget } from '../../../components/chat/filePreviewContext'
  import FilePreviewPanel from '../../../components/chat/FilePreviewPanel.svelte'
  import MobileFilePreviewScreen from './MobileFilePreviewScreen.svelte'
  let { target, remember }: { target: FilePreviewTarget; remember: (file: FilePreviewTarget) => void } = $props()
  const panel = useSessionPanel()!
  const stack = useMobileStack()!
  let child = $state.raw<FilePreviewTarget>()
  function openFile(next: FilePreviewTarget) {
    child = next
    remember(next)
    stack.open({ dismiss: () => (panel.file = target), title: next.path, content: nested })
  }
</script>

{#snippet nested()}{#if child}<MobileFilePreviewScreen target={child} {remember} />{/if}{/snippet}
<div class="flex h-full min-w-0 flex-col">
  <FilePreviewPanel {target} {openFile} onclose={() => stack.back()} />
</div>
