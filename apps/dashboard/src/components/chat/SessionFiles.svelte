<script lang="ts">
  import { onMount } from 'svelte'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { listSessionFiles } from '../../api/sessionFiles'
  import type { SharedFile } from '../../api/types'
  import { subscribeDashboardEvents } from '../../services/eventStream'
  import SharedFileCard from './SharedFileCard.svelte'

  let { sessionId }: { sessionId: string } = $props()
  let files = $state<SharedFile[]>([])
  let loading = $state(true)
  let error = $state<string | null>(null)
  let controller: AbortController

  function merge(incoming: SharedFile[]) {
    const entries = new Map(files.map(file => [file.file_id, file]))
    for (const file of incoming) entries.set(file.file_id, file)
    files = [...entries.values()].sort((a, b) => b.created_at.localeCompare(a.created_at) || b.file_id.localeCompare(a.file_id))
  }

  async function load() {
    loading = true
    error = null
    try {
      let cursor: string | undefined
      do {
        const page = await listSessionFiles(sessionId, cursor, controller.signal)
        merge(page.files)
        cursor = page.next_cursor ?? undefined
      } while (cursor)
    } catch (cause) {
      if (!controller.signal.aborted) error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      loading = false
    }
  }

  onMount(() => {
    controller = new AbortController()
    const unsubscribe = subscribeDashboardEvents(event => {
      if (event.kind === 'session_event' && event.event.session_id === sessionId && event.event.file) merge([event.event.file])
    })
    void load()
    return () => { controller.abort(); unsubscribe() }
  })
</script>

<Collapsible.Root class="mx-auto w-full max-w-[760px] px-4">
  <Collapsible.Trigger class="rounded-md border px-3 py-2 text-sm font-medium">Shared files ({files.length}{loading ? '…' : ''})</Collapsible.Trigger>
  <Collapsible.Content class="mt-3 space-y-3">
    {#if error}
      <p role="alert" class="text-sm text-destructive">{error}</p>
      <Button variant="outline" onclick={load}>Retry</Button>
    {/if}
    {#each files as file (file.file_id)}<SharedFileCard {file} {sessionId} />{/each}
  </Collapsible.Content>
</Collapsible.Root>
