<script lang="ts">
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import type { DeviceRuntime } from '../../../device/runtime'
  import { useSessionPanel } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import { displayChangeGroup, groupChanges, splitChangePath, changeDirLabel, shortSha } from '../../../components/chat/git/gitChanges'
  import GitPanel from '../../../components/chat/git/GitPanel.svelte'
  import MobileGitDiffScreen from './MobileGitDiffScreen.svelte'
  let { device, workspaceId }: { device: DeviceRuntime; workspaceId: string } = $props()
  const git = useSessionPanel()!.git
  const stack = useMobileStack()!
  let changeFiles = $state<import('../../../api/types').GitChangedFile[]>([])
  let commitFiles = $state<import('../../../api/types').GitCommitFile[]>([])
</script>

{#snippet diff()}{#if git.diff}<MobileGitDiffScreen {device} {workspaceId} target={{ file: git.diff }} files={changeFiles} />{/if}{/snippet}
{#snippet commitDiff()}{#if git.commit && git.commitFile}<MobileGitDiffScreen {device} {workspaceId} target={{ sha: git.commit, file: git.commitFile }} files={commitFiles} />{/if}{/snippet}
<GitPanel {device} {workspaceId} {git}
  onstatuserror={(error) => toast.error(error.message, { duration: 6000 })}
  onopen={(file, files) => {
    git.diff = file
    changeFiles = groupChanges(files).flatMap(group => group.files)
    const group = displayChangeGroup(file.group)
    stack.open({
      dismiss: () => (git.diff = null), content: diff, title: splitChangePath(file.path).name,
      subtitle: [file.status, $t(group.label), changeDirLabel(file)].filter(Boolean).join(' · '),
    })
  }}
  oncommitfile={(sha, file, files) => {
    git.commit = sha
    git.commitFile = file
    commitFiles = files
    stack.open({
      dismiss: () => (git.commitFile = null), content: commitDiff, title: splitChangePath(file.path).name,
      subtitle: [file.status, shortSha(sha), changeDirLabel(file)].filter(Boolean).join(' · '),
    })
  }}
/>
