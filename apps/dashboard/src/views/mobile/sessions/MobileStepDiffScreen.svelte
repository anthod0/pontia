<script lang="ts">
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../../device/context'
  import MobileGitDiffScreen from './MobileGitDiffScreen.svelte'
  let {
    path,
    workspace,
    workspaceId,
    patch,
  }: { path: string; workspace: string | null; workspaceId: string | null; patch: string } = $props()
  const device = useDeviceRuntime()
  const changes = device.workspaceQueries.createWorkspaceGitChangesQuery(
    () => workspaceId ?? '',
    () => !patch && !!workspaceId
  )
  const absolute = $derived(path.startsWith('/') ? path : (workspace ? workspace.replace(/\/$/, '') + '/' : '') + path)
  const file = $derived(changes.data?.files.find((f) => changes.data?.repo_root + '/' + f.path === absolute))
</script>

{#if patch}
  <div class="h-full overflow-auto font-mono text-11 leading-5">
    <pre class="min-w-max">{#each patch.split('\n') as line, i (i)}<span class={['block',line.startsWith('+') && 'bg-success/12',line.startsWith('-') && 'bg-destructive/12']}>{line || ' '}</span>{/each}</pre>
  </div>
{:else if !workspaceId}<p>{$t('git.noWorkspaceDescription')}</p>
{:else if changes.isError}<p role="alert">{changes.error.message}</p>
{:else if !changes.data}<p role="status">{$t('common.loading')}</p>
{:else if file}<MobileGitDiffScreen {device} {workspaceId} target={{ file }} files={changes.data.files} />
{:else}<p>{$t('git.noTextChanges')}</p>{/if}
