<script lang="ts">
  import SidePanel from '$lib/components/ui/overlay/SidePanel.svelte'
  import { t } from '$lib/i18n'
  import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { sessionRecentFiles, withoutSessionRecentFiles } from '$lib/sessionRecentFiles'
  import LocalhostPreviewPanel from '../LocalhostPreviewPanel.svelte'
  import FilePreviewPanel from '../FilePreviewPanel.svelte'
  import FilePreviewResizer from '../FilePreviewResizer.svelte'
  import RecentFileList from '../files/RecentFileList.svelte'
  import GitPanel from '../git/GitPanel.svelte'
  import { workspaceTitle } from '../sessionMetadata'
  import type { SessionPanel, SessionPanelSource, SessionPanelTab } from './sessionPanelContext.svelte'

  let { panel, source }: { panel: SessionPanel; source: SessionPanelSource } = $props()

  // The panel is recreated for each device.
  // svelte-ignore state_referenced_locally
  const device = source.device
  // svelte-ignore state_referenced_locally
  const recentFiles = panel.recentFiles(device)
  const sessionFiles = $derived(sessionRecentFiles(recentFiles.current, source.sessionId))
  const workspacesQuery = device.workspaceQueries.createWorkspacesQuery()
  const workspace = $derived(workspacesQuery.data?.find(item => item.workspace_id === source.workspaceId))
  let element = $state<HTMLElement | null>(null)
  let git = $state<GitPanel>()

  // Opening the panel by hand moves focus into it, so the keyboard works there right away.
  $effect(() => {
    if (!panel.focusPending || !element) return
    panel.focusPending = false
    element.focus()
  })
</script>

<SidePanel open={panel.open} close={() => panel.close()} bind:ref={element} tabindex={-1} data-session-panel aria-label={$t('nav.sidebar')} onkeydown={(event) => git?.keydown(event)}>
  <FilePreviewResizer />
  <Tabs.Root bind:value={() => panel.tab, next => { panel.tab = next as SessionPanelTab }} class="min-h-0 flex-1 gap-0">
    <header class="flex h-12 shrink-0 items-center justify-between gap-1 border-b px-2">
      <Tabs.List variant="line" aria-label={$t('nav.sidebar')} class="h-full gap-0">
        <Tabs.Trigger value="files" class="h-full px-2.5"><FileTextIcon />{$t('files.title')}</Tabs.Trigger>
        <Tabs.Trigger value="preview" class="h-full px-2.5"><GlobeIcon />{$t('preview.title')}</Tabs.Trigger>
        <Tabs.Trigger value="git" class="h-full px-2.5"><GitBranchIcon />{$t('git.title')}</Tabs.Trigger>
      </Tabs.List>
      <Button variant="ghost" size="icon-sm" class="rounded-full p-0 max-lg:size-11" aria-label={$t('common.close')} title={$t('common.close')} onclick={() => panel.close()}><XIcon class="size-4 max-lg:size-5" /></Button>
    </header>
    <Tabs.Content value="preview" class="flex min-h-0 flex-col">
      <LocalhostPreviewPanel bind:target={panel.preview} access={device.access} />
    </Tabs.Content>
    <Tabs.Content value="files" class="flex min-h-0 flex-col">
      {#if panel.file}
        {#key panel.file}<FilePreviewPanel target={panel.file} onclose={() => panel.closeFile()} />{/key}
      {:else if sessionFiles.length}
        <RecentFileList files={sessionFiles} onopen={file => panel.openFile({ device, ...file })}
          onclear={() => { recentFiles.current = withoutSessionRecentFiles(recentFiles.current, source.sessionId) }} />
      {:else}
        <Empty.Root class="min-h-0 p-8">
          <Empty.Header>
            <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FileTextIcon class="size-6" /></Empty.Media>
            <Empty.Title>{$t('files.noneOpen')}</Empty.Title>
            <Empty.Description>{$t('files.noneOpenDescription')}</Empty.Description>
          </Empty.Header>
        </Empty.Root>
      {/if}
    </Tabs.Content>
    <Tabs.Content value="git" class="flex min-h-0 flex-col">
      <!-- Git is read only while the tab shows. -->
      {#if panel.tab === 'git' && source.workspaceId}
        {#key source.workspaceId}
          <GitPanel bind:this={git} {device} workspaceId={source.workspaceId} git={panel.git} hint={$t('git.keyboardHint')}>
            {#snippet workspaceHeading()}
                <FolderIcon class="size-4 shrink-0 text-muted-foreground" />
                <span class="min-w-0 flex-1 truncate text-sm font-medium text-heading" title={workspace?.canonical_path}>{workspace ? workspaceTitle(workspace) : ''}</span>
            {/snippet}
          </GitPanel>
        {/key}
      {:else if !source.workspaceId}
        <Empty.Root class="min-h-0 p-8">
          <Empty.Header>
            <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FolderIcon class="size-6" /></Empty.Media>
            <Empty.Title>{$t('workspaces.noWorkspace')}</Empty.Title>
            <Empty.Description>{$t('git.noWorkspaceDescription')}</Empty.Description>
          </Empty.Header>
        </Empty.Root>
      {/if}
    </Tabs.Content>
  </Tabs.Root>
</SidePanel>
