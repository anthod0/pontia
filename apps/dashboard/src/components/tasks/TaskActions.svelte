<script lang="ts">
  import { CircleAlert } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import type { TaskView } from '../../api/types'
  import { cancelTask, createHumanSignal, interruptTask, pauseTask, resumeTask } from '../../stores/tasks'

  let { task }: { task: TaskView } = $props()

  let signalKind = $state('needs_input')
  let signalSummary = $state('')
  let signalDetail = $state('')
  let signalSeverity = $state<'low' | 'medium' | 'high'>('medium')
  let busy = $state(false)
  let error = $state<string | null>(null)

  async function run(action: () => Promise<unknown>) {
    busy = true
    error = null
    try {
      await action()
    } catch (err) {
      error = err instanceof Error ? err.message : String(err)
    } finally {
      busy = false
    }
  }

  function submitSignal() {
    const summary = signalSummary.trim()
    if (!summary) return
    void run(async () => {
      await createHumanSignal(task.task_id, {
        kind: signalKind.trim() || 'needs_input',
        summary,
        detail: signalDetail.trim() || null,
        severity: signalSeverity,
      })
      signalSummary = ''
      signalDetail = ''
    })
  }
</script>

<div class="space-y-4">
  {#if error}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Task action failed</Alert.Title>
      <Alert.Description>{error}</Alert.Description>
    </Alert.Root>
  {/if}

  <div class="flex flex-wrap gap-2">
    <Button size="sm" variant="outline" disabled={busy} onclick={() => void run(() => pauseTask(task.task_id))}>Pause</Button>
    <Button size="sm" variant="outline" disabled={busy} onclick={() => void run(() => resumeTask(task.task_id))}>Resume</Button>
    <Button size="sm" variant="outline" disabled={busy} onclick={() => void run(() => interruptTask(task.task_id))}>Interrupt</Button>
    <Button size="sm" variant="destructive" disabled={busy} onclick={() => void run(() => cancelTask(task.task_id))}>Cancel</Button>
  </div>

  <div class="grid gap-4">
    <div class="space-y-3 rounded-lg border p-4">
      <div class="grid gap-3 sm:grid-cols-2">
        <div class="space-y-2">
          <Label for="signal-kind">Signal kind</Label>
          <Input id="signal-kind" bind:value={signalKind} />
        </div>
        <div class="space-y-2">
          <Label for="signal-severity">Severity</Label>
          <select id="signal-severity" bind:value={signalSeverity} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm">
            <option value="low">low</option>
            <option value="medium">medium</option>
            <option value="high">high</option>
          </select>
        </div>
      </div>
      <div class="space-y-2">
        <Label for="signal-summary">Human signal</Label>
        <Input id="signal-summary" bind:value={signalSummary} placeholder="Short summary" />
      </div>
      <Textarea bind:value={signalDetail} placeholder="Optional detail" />
      <Button size="sm" disabled={busy || !signalSummary.trim()} onclick={submitSignal}>Raise human signal</Button>
    </div>
  </div>
</div>
