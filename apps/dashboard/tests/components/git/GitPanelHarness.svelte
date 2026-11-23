<script lang="ts">
  import { provideFilePreview } from '../../../src/components/chat/filePreviewContext'
  import { provideGitPanel } from '../../../src/components/chat/git/gitPanelContext.svelte'
  import GitPanel from '../../../src/components/chat/git/GitPanel.svelte'
  import { testDevice } from '../../deviceFixture'

  let { onpanel }: { onpanel?: (panel: ReturnType<typeof provideGitPanel>) => void } = $props()
  const panel = provideGitPanel(provideFilePreview())
  panel.attach({ device: testDevice, workspaceId: 'workspace-1' })
  // svelte-ignore state_referenced_locally
  onpanel?.(panel)
</script>

<button type="button" onclick={() => panel.toggle()}>toggle</button>
{#if panel.open && panel.source}
  <GitPanel {panel} source={panel.source} />
{/if}
