<script lang="ts">
  import { t } from '$lib/i18n'
  import { statusLabel } from '$lib/i18n/status'
  import { sessionStateDotClass } from '$lib/sessionState'

  let { state }: { state: string } = $props()
</script>

{#if state !== 'exited'}
  <span role="img" aria-label={$t('sessions.state', { state: statusLabel(state, $t) })}
    class={`size-2 shrink-0 rounded-full ${sessionStateDotClass(state)}`}></span>
{/if}
