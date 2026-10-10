<script lang="ts">
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { modelPickerDisabledReason } from '$lib/modelControls'
  import { setSessionModel } from '../../api/client'
  import type { SessionView } from '../../api/types'
  import { createSessionModelsQuery } from '../../queries/sessions'

  let { session, onClose }: { session: SessionView; onClose: () => void } = $props()
  const modelsQuery = createSessionModelsQuery(() => session.session_id)
  let query = $state('')
  let submitting = $state(false)
  let pendingModel = $state<string | null>(null)
  let error = $state<string | null>(null)
  const currentModel = $derived(session.model ?? modelsQuery.data?.current_model)
  const unavailable = $derived(modelPickerDisabledReason(session))
  const readonly = $derived(session.capabilities.set_model !== true)
  const models = $derived(modelsQuery.data?.models.filter((model) => `${model.name} ${model.id}`.toLowerCase().includes(query.trim().toLowerCase())) ?? [])

  $effect(() => {
    if (pendingModel && session.model === pendingModel) onClose()
  })

  async function choose(model: string): Promise<void> {
    if (!modelsQuery.data || readonly || unavailable || submitting || pendingModel) return
    submitting = true
    error = null
    try {
      await setSessionModel(session.session_id, model, modelsQuery.data.runtime_id)
      pendingModel = model
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      submitting = false
    }
  }
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open) onClose() }}>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Choose model</Dialog.Title>
      <Dialog.Description>{readonly ? 'Available models for this session. Model changes are not supported.' : 'Applies to subsequent turns in this session.'}</Dialog.Description>
    </Dialog.Header>
    <Input aria-label="Search models" placeholder="Search models…" bind:value={query} />
    {#if currentModel}<p class="text-xs text-muted-foreground">Current model: {currentModel}</p>{/if}
    {#if unavailable}<p role="status" class="text-sm text-muted-foreground">{unavailable}</p>{/if}
    {#if error || modelsQuery.error}<p role="alert" class="text-sm text-destructive">{error ?? modelsQuery.error?.message}</p>{/if}
    {#if modelsQuery.isPending}
      <p role="status">Loading models…</p>
    {:else if pendingModel}
      <p role="status">Model change requested. Waiting for confirmation from the agent…</p>
    {:else if modelsQuery.data}
      <ul aria-label="Available models" class="max-h-80 space-y-1 overflow-y-auto">
        {#each models as model (model.id)}
          <li>
            <Button variant="ghost" class="h-auto w-full justify-start gap-3 px-3 py-2 text-left" aria-label={model.name} disabled={readonly || Boolean(unavailable) || submitting || model.id === currentModel} onclick={() => void choose(model.id)}>
              <span class="min-w-0 flex-1 whitespace-normal">
                <span class="block font-medium">{model.name}</span>
                <span class="block text-xs text-muted-foreground">{model.id}</span>
                {#if model.description}<span class="block text-xs text-muted-foreground">{model.description}</span>{/if}
              </span>
              {#if model.id === currentModel}<CheckIcon class="size-4 shrink-0" aria-label="Current model" />{/if}
            </Button>
          </li>
        {:else}
          <li class="py-3 text-sm text-muted-foreground">No models found.</li>
        {/each}
      </ul>
    {/if}
  </Dialog.Content>
</Dialog.Root>
