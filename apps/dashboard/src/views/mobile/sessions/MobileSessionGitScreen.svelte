<script lang="ts">
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
  let statusError = $state<Error>()
  let panel = $state<GitPanel>()
</script>

{#snippet diff()}{#if git.diff}<MobileGitDiffScreen {device} {workspaceId} target={{ file: git.diff }} files={changeFiles} />{/if}{/snippet}
{#snippet commitDiff()}{#if git.commit && git.commitFile}<MobileGitDiffScreen {device} {workspaceId} target={{ sha: git.commit, file: git.commitFile }} files={commitFiles} />{/if}{/snippet}
<GitPanel bind:this={panel} {device} {workspaceId} {git}
  onstatuserror={(error) => (statusError = error)}
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
{#if statusError}<div role="alert" class="absolute inset-x-3 top-14 z-10 flex items-center gap-2 rounded-item border border-destructive/30 bg-background px-3 py-2 text-13 text-destructive">
    <span class="min-w-0 flex-1 break-words">{statusError.message}</span>
    <button class="min-h-11 shrink-0 font-medium" onclick={() => { statusError = undefined; void panel?.refresh() }}>{$t('common.retry')}</button>
  </div>{/if}
