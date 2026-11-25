<script lang="ts">
  import MessageResponse from '../../src/lib/components/ai-elements/message/message-response.svelte'
  import { provideFilePreviewNavigator } from '../../src/components/chat/filePreviewContext'
  import { provideTopBarAction } from '../../src/components/layout/topBarAction'
  import { provideSessionPanel } from '../../src/components/chat/sessionPanel/sessionPanelContext.svelte'
  import SessionPanelSession from '../../src/components/chat/sessionPanel/SessionPanelSession.svelte'
  import SessionSidePanel from '../../src/components/chat/sessionPanel/SessionSidePanel.svelte'
  import type { SessionPanel } from '../../src/components/chat/sessionPanel/sessionPanelContext.svelte'

  /** The session side panel as the app shell and a session page put it together. */
  let { sessionId = 'session-1', workspaceId = 'workspace-1', onpanel }: {
    sessionId?: string
    workspaceId?: string | null
    onpanel?: (panel: SessionPanel) => void
  } = $props()
  const action = provideTopBarAction()
  const panel = provideSessionPanel()
  provideFilePreviewNavigator({ open: target => panel.openFile(target) })
  // svelte-ignore state_referenced_locally
  onpanel?.(panel)
</script>

{@render $action?.()}
{#key sessionId}
  <SessionPanelSession {sessionId} {workspaceId} />
  <MessageResponse content="Read [notes](/work/notes.md)." markdown {sessionId} />
{/key}
<div class="file-preview-layout">
  {#if panel.source}
    {#key panel.source.device}<SessionSidePanel {panel} source={panel.source} />{/key}
  {/if}
</div>
