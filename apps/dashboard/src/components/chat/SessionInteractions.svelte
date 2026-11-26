<script lang="ts">
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { createInteractions } from '../../features/sessions/interactions.svelte'
  let { sessionId }: { sessionId: string } = $props()
  const model = createInteractions(() => sessionId)
</script>

{#if model.interactions.length}
  <section aria-label={$t('interaction.title')} class="mx-auto w-full max-w-[760px] px-4 pb-4">
    {#each model.interactions as interaction (interaction.interaction_id)}
      <Alert.Root class="mb-3">
        <Alert.Title>{interaction.title}</Alert.Title>
        <Alert.Description>
          {#if interaction.description}<p class="whitespace-pre-wrap break-words">{interaction.description}</p>{/if}
          {#if interaction.state === 'pending'}
            <div class="mt-3 flex flex-wrap gap-2">
              {#each interaction.options as option (option.key)}
                <Button class="h-auto max-w-full whitespace-normal py-1.5 text-left [overflow-wrap:anywhere]" variant={option.intent === 'positive' ? 'default' : 'destructive'} disabled={model.submitting.has(interaction.interaction_id)} onclick={() => model.respond(interaction, option.key)}>{option.label}</Button>
              {/each}
            </div>
          {:else if interaction.state === 'resolved'}
            <p role="status">{$t('interaction.responseSent')} {interaction.options.find(o => o.key === (interaction.response && 'option_key' in interaction.response ? interaction.response.option_key : undefined))?.label}</p>
          {:else if interaction.state === 'expired'}
            <p role="status">{$t('interaction.unavailable')}</p>
          {:else}
            <p role="status">{$t('interaction.sending')}</p>
          {/if}
        </Alert.Description>
      </Alert.Root>
    {/each}
  </section>
{/if}
