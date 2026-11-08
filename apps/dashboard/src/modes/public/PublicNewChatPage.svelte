<script lang="ts">
  import { onMount } from 'svelte'
  import type { WorkspaceView } from '../../api/types'
  import type { DeviceRuntime } from '../../device/runtime'
  import type { ConnectedDevice } from './deviceConnections'
  import { deviceStateLabel, useDeviceDirectory } from './deviceDirectory'
  import DeviceProvider from '../../device/DeviceProvider.svelte'
  import ChatShortcuts from '../../components/chat/ChatShortcuts.svelte'
  import NewChatPage from '../../pages/NewChatPage.svelte'
  import * as Select from '$lib/components/ui/select/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { navigate } from '$lib/navigation'
  import { workspaceTitle } from '../../components/chat/sessionMetadata'

  let { devices, loading, error, onRetry, onRetryDevice, signInUrl }: {
    devices: ConnectedDevice[]
    loading: boolean
    error: string | null
    onRetry: () => void
    onRetryDevice: (handle: string) => void
    signInUrl?: string
  } = $props()

  type WorkspaceGroup = {
    handle: string
    runtime: DeviceRuntime
    workspaces: WorkspaceView[]
    pending: boolean
    error: Error | null
    retry: () => void
  }
  let groups = $state<WorkspaceGroup[]>([])
  let selectedKey = $state('')
  let preferredKey = $state('')
  let preferenceLoaded = $state(false)
  const directory = useDeviceDirectory()
  const storageKey = 'pontia.chat.lastDeviceWorkspace'
  const choiceKey = (handle: string, workspaceId: string) => JSON.stringify([handle, workspaceId])
  const choices = $derived(groups.filter(group => devices.some(device => device.handle === group.handle && device.state === 'online' && device.runtime === group.runtime)).flatMap(group => group.workspaces.map(workspace => ({
    key: choiceKey(group.handle, workspace.workspace_id),
    handle: group.handle,
    name: devices.find(device => device.handle === group.handle)!.name,
    runtime: group.runtime,
    workspace,
  }))))
  const selected = $derived(choices.find(choice => choice.key === selectedKey))
  const pending = $derived(!preferenceLoaded || loading || devices.some(device => device.state === 'connecting') || groups.some(group => group.pending))

  $effect(() => {
    const online = devices.filter(device => device.state === 'online' && device.runtime)
    const results = new Map<string, WorkspaceGroup>()
    groups = []
    const unsubscribers = online.map(device => {
      const runtime = device.runtime!
      const store = runtime.workspaceQueries.createWorkspacesStore()
      return store.subscribe(result => {
        results.set(device.handle, {
          handle: device.handle, runtime, workspaces: result.data ?? [],
          pending: result.isPending, error: result.error,
          retry: () => void result.refetch(),
        })
        groups = [...results.values()]
      })
    })
    return () => unsubscribers.forEach(unsubscribe => unsubscribe())
  })

  $effect(() => {
    if (!preferenceLoaded || choices.some(choice => choice.key === selectedKey)) return
    const preferred = choices.find(choice => choice.key === preferredKey)
    if (preferred) selectedKey = preferred.key
    else if (!pending || !preferredKey) selectedKey = choices[0]?.key ?? ''
  })

  function syncQuery(): void {
    const query = new URLSearchParams(window.location.search)
    const handle = query.get('device')
    const workspaceId = query.get('workspace')
    if (handle && workspaceId) {
      preferredKey = choiceKey(handle, workspaceId)
      selectedKey = ''
    }
  }

  function selectWorkspace(key: string): void {
    selectedKey = key
    preferredKey = key
    const choice = choices.find(choice => choice.key === key)
    if (choice) directory.rememberDevice(choice.handle)
    try { window.localStorage.setItem(storageKey, key) } catch { /* Storage is optional. */ }
  }

  onMount(() => {
    try { preferredKey = window.localStorage.getItem(storageKey) ?? '' } catch { /* Storage is optional. */ }
    syncQuery()
    preferenceLoaded = true
  })
</script>

<svelte:window onpopstate={syncQuery} />

{#snippet workspaceSelector(disabled: boolean)}
  <Select.Root type="single" value={selectedKey} onValueChange={selectWorkspace} {disabled}>
    <Select.Trigger aria-label="Workspace" class="h-[34px] min-w-0 max-w-80 bg-background text-[13px]">
      <span class="truncate">{selected ? `${workspaceTitle(selected.workspace)} · ${selected.name}` : 'Choose a workspace'}</span>
    </Select.Trigger>
    <Select.Content class="max-h-80 w-80">
      {#each choices as choice (choice.key)}
        <Select.Item value={choice.key} label={`${workspaceTitle(choice.workspace)} · ${choice.name}`}>
          <div class="min-w-0">
            <span class="block truncate">{workspaceTitle(choice.workspace)} · {choice.name}</span>
            <span class="block truncate font-mono text-xs text-muted-foreground">{choice.workspace.display_path}</span>
          </div>
        </Select.Item>
      {/each}
    </Select.Content>
  </Select.Root>
{/snippet}

{#if error}
  <div role="alert" class="flex items-center gap-3 py-4 text-sm">
    <span>{error}</span>
    {#if signInUrl}<Button href={signInUrl}>Sign in</Button>
    {:else}<Button variant="outline" onclick={onRetry}>Try again</Button>{/if}
  </div>
{/if}
{#each groups.filter(group => group.error) as group (group.handle)}
  <div role="alert" class="flex items-center gap-3 py-2 text-sm">
    Could not load workspaces on {devices.find(device => device.handle === group.handle)?.name}.
    <Button variant="outline" onclick={group.retry}>Retry workspaces</Button>
  </div>
{/each}

{#if selected}
  {#key selected.runtime}
    <DeviceProvider device={selected.runtime}>
      <ChatShortcuts newChatHandle={selected.handle} />
      <NewChatPage createWorkspaceId={selected.workspace.workspace_id} {workspaceSelector} routeScope={`/${selected.handle}`} />
    </DeviceProvider>
  {/key}
{:else}
  <Empty.Root class="min-h-[60svh]">
    <Empty.Header>
      <Empty.Title>{pending ? 'Loading workspaces' : 'No workspaces available'}</Empty.Title>
      <Empty.Description>Choose an online device to set up a workspace and start a session.</Empty.Description>
    </Empty.Header>
    <Empty.Content class="max-w-md">
      {#each devices as device (device.handle)}
        <div class="flex w-full items-center gap-3 text-sm">
          <span class="min-w-0 flex-1 truncate text-left">{device.name} · {deviceStateLabel[device.state]}</span>
          {#if device.state === 'online'}
            <Button variant="outline" onclick={() => void navigate('/settings/workspaces', undefined, { scope: `/${device.handle}` })}>Set up workspaces</Button>
          {:else if device.state === 'offline'}
            <Button variant="outline" onclick={() => onRetryDevice(device.handle)}>Retry {device.name}</Button>
          {/if}
        </div>
      {/each}
      {#if !loading && !devices.length && !error}<Button href="https://pontia.dev">Manage devices</Button>{/if}
    </Empty.Content>
  </Empty.Root>
{/if}
