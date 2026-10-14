<script lang="ts">
  import { onMount } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { getInteraction, listPendingInteractions, respondInteraction } from '../../api/client'
  import { ApiError } from '../../api/errors'
  import type { Interaction } from '../../api/types'
  import { subscribeDashboardEvents } from '../../services/eventStream'
  import { sseStatus } from '../../stores/connection'

  export let sessionId: string
  let interactions: Interaction[] = []
  let submitting = new Set<string>()
  let error: string | null = null
  let mounted = false
  let generation = 0
  let activeSession = ''
  let refreshRunning = false
  let refreshQueued = false

  async function refresh() {
    refreshQueued = true
    if (refreshRunning) return
    refreshRunning = true
    try {
      while (refreshQueued && mounted) {
        refreshQueued = false
        const current = generation
        const session = sessionId
        if (!session) continue
        try {
          const snapshot = await listPendingInteractions(session)
          const resolving = interactions.filter(i => i.state === 'resolving')
          const updates = await Promise.all(resolving.map(i => getInteraction(session, i.interaction_id)))
          if (current === generation) {
            for (const update of updates) {
              if (interactions.find(i => i.interaction_id === update.interaction_id)?.state === 'resolving') recordResult(update)
            }
            const outcomes = interactions.filter(i => i.state !== 'pending')
            interactions = [...snapshot.filter(i => !outcomes.some(o => o.interaction_id === i.interaction_id)), ...outcomes]
            error = null
          }
        } catch (cause) {
          if (current === generation) error = cause instanceof Error ? cause.message : String(cause)
        }
      }
    } finally { refreshRunning = false }
  }

  function recordResult(result: Interaction) {
    const index = interactions.findIndex(i => i.interaction_id === result.interaction_id)
    interactions = index < 0 ? [...interactions, result] : interactions.map(i => i.interaction_id === result.interaction_id ? result : i)
  }

  async function respond(interaction: Interaction, key: string) {
    if (submitting.has(interaction.interaction_id)) return
    const current = generation
    const session = sessionId
    submitting = new Set(submitting).add(interaction.interaction_id)
    error = null
    try {
      const result = await respondInteraction(session, interaction.interaction_id, key)
      if (current === generation) recordResult(result)
    } catch (cause) {
      if (current !== generation) return
      if (cause instanceof ApiError && cause.status === 409) {
        try {
          const result = await getInteraction(session, interaction.interaction_id)
          if (current === generation) recordResult(result)
        } catch (readError) { if (current === generation) error = readError instanceof Error ? readError.message : String(readError) }
      } else error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      if (current === generation) { submitting = new Set(submitting); submitting.delete(interaction.interaction_id) }
    }
  }

  $: if (mounted && sessionId !== activeSession) {
    activeSession = sessionId
    generation += 1
    interactions = []
    submitting = new Set()
    error = null
    void refresh()
  }
  $: if (mounted && $sseStatus === 'open') void refresh()

  onMount(() => {
    mounted = true
    const unsubscribe = subscribeDashboardEvents(event => {
      if (event.kind === 'interactions_resync' || (event.kind === 'interaction_changed' && event.session_id === sessionId)) void refresh()
    })
    const visible = () => { if (document.visibilityState === 'visible') void refresh() }
    document.addEventListener('visibilitychange', visible)
    window.addEventListener('pageshow', visible)
    return () => {
      mounted = false
      generation += 1
      unsubscribe()
      document.removeEventListener('visibilitychange', visible)
      window.removeEventListener('pageshow', visible)
    }
  })
</script>

{#if interactions.length || error}
  <section aria-label="Agent interactions" class="mx-auto w-full max-w-[760px] px-4 pb-4">
    {#each interactions as interaction (interaction.interaction_id)}
      <Alert.Root class="mb-3">
        <Alert.Title>{interaction.title}</Alert.Title>
        <Alert.Description>
          {#if interaction.description}<p class="whitespace-pre-wrap break-words">{interaction.description}</p>{/if}
          {#if interaction.state === 'pending'}
            <div class="mt-3 flex flex-wrap gap-2">
              {#each interaction.options as option (option.key)}
                <Button variant={option.intent === 'positive' ? 'default' : 'destructive'} disabled={submitting.has(interaction.interaction_id)} onclick={() => respond(interaction, option.key)}>{option.label}</Button>
              {/each}
            </div>
          {:else if interaction.state === 'resolved'}
            <p role="status">Response sent: {interaction.options.find(o => o.key === interaction.response?.option_key)?.label}</p>
          {:else if interaction.state === 'expired'}
            <p role="status">This interaction is no longer available.</p>
          {:else}
            <p role="status">A response is being sent.</p>
          {/if}
        </Alert.Description>
      </Alert.Root>
    {/each}
    {#if error}<Alert.Root variant="destructive"><Alert.Description>{error}</Alert.Description></Alert.Root>{/if}
  </section>
{/if}
