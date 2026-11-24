<script lang="ts">
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { createInteractions } from '../../../features/sessions/interactions.svelte'
  let { sessionId }: { sessionId: string } = $props()
  const model = createInteractions(() => sessionId)
  const pending = $derived(model.interactions.filter((i) => i.state === 'pending' || i.state === 'resolving'))
</script>

{#if pending.length}
  <section aria-label={$t('interaction.title')} class="mx-auto w-full max-w-[760px] max-h-[32dvh] overflow-auto">
    {#each pending as interaction (interaction.interaction_id)}
      <Alert.Root class="mb-1">
        <Alert.Title>{interaction.title}</Alert.Title>
        <Alert.Description>
          {#if interaction.description}<p class="whitespace-pre-wrap break-words">
              {interaction.description}
            </p>{/if}
          {#if interaction.state === 'pending'}
            <div class="mt-3 flex flex-col gap-2">
              {#each interaction.options as option, index (option.key)}
                <Button
                  class="min-h-11 justify-start max-w-full whitespace-normal py-1.5 text-left [overflow-wrap:anywhere]"
                  variant={option.intent === 'positive' ? 'default' : 'destructive'}
                  disabled={model.submitting.has(interaction.interaction_id)}
                  onclick={() => model.respond(interaction, option.key)}>{index + 1}. {option.label}</Button
                >
              {/each}
            </div>
          {:else}
            <p role="status">{$t('interaction.sending')}</p>
          {/if}
        </Alert.Description>
      </Alert.Root>
    {/each}
  </section>
{/if}
