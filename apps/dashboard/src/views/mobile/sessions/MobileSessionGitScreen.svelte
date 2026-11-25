<script lang="ts">
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import type { DeviceRuntime } from '../../../device/runtime'
  import { useSessionPanel } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import { gitChangeGroups, splitChangePath, changeDirLabel, shortSha } from '../../../components/chat/git/gitChanges'
  import GitPanel from '../../../components/chat/git/GitPanel.svelte'
  import MobileGitCommitScreen from './MobileGitCommitScreen.svelte'
  import MobileGitDiffScreen from './MobileGitDiffScreen.svelte'
  let { device, workspaceId }: { device: DeviceRuntime; workspaceId: string } = $props()
  const git = useSessionPanel()!.git
  const stack = useMobileStack()!
</script>

{#snippet diff()}{#if git.diff}<MobileGitDiffScreen {device} {workspaceId} target={{ file: git.diff }} />{/if}{/snippet}
{#snippet commit()}{#if git.commit}<MobileGitCommitScreen {device} {workspaceId} sha={git.commit} />{/if}{/snippet}
<GitPanel {device} {workspaceId} {git}
  onstatuserror={(error) => toast.error(error.message)}
  onopen={(file) => {
    git.diff = file
    const group = gitChangeGroups.find(item => item.group === file.group)!
    stack.open({
      dismiss: () => (git.diff = null), content: diff, title: splitChangePath(file.path).name,
      subtitle: [file.status, $t(group.label), changeDirLabel(file)].filter(Boolean).join(' · '),
    })
  }}
  oncommit={(sha) => {
    git.openCommit(sha)
    stack.open({ dismiss: () => (git.commit = null), title: $t('git.commitTitle', { sha: shortSha(sha) }), content: commit })
  }}
/>
