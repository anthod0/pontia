<script lang="ts">
  import { useSessionPanel } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import type { DeviceRuntime } from '../../../device/runtime'
  import GitCommitDetail from '../../../components/chat/git/GitCommitDetail.svelte'
  import MobileGitDiffScreen from './MobileGitDiffScreen.svelte'
  let { device, workspaceId, sha }: { device: DeviceRuntime; workspaceId: string; sha: string } = $props()
  const git = useSessionPanel()!.git
  const stack = useMobileStack()!
</script>

{#snippet diff()}{#if git.commitFile}<MobileGitDiffScreen {device} {workspaceId} target={{ sha, file: git.commitFile }} />{/if}{/snippet}
<div class="flex h-full min-w-0 flex-col">
  <GitCommitDetail
    {device}
    {workspaceId}
    {sha}
    onopen={(next) => {
      git.commitFile = next
      stack.open({ dismiss: () => (git.commitFile = null), content: diff, title: next.path })
    }}
  />
</div>
