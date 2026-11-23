<script lang="ts">
  import FileDropZone from '../../src/components/chat/FileDropZone.svelte'
  import MessageComposer from '../../src/components/chat/MessageComposer.svelte'

  let { draftId, busy = false }: { draftId: string; busy?: boolean } = $props()
</script>

<FileDropZone>
  <p>Conversation feed</p>
  <aside data-bot-chat-panel><p>Bot side panel</p></aside>
  <MessageComposer value="" {draftId} {busy} onSubmit={() => {}} />
</FileDropZone>
<nav>Sidebar</nav>
