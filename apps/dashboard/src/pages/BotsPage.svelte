<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { navigate } from '$lib/navigation'
  import MobileBotsPage from '../views/mobile/MobileBotsPage.svelte'

  $effect(() => {
    if (!isMobileShell()) void navigate('/bot', undefined, { replaceState: true })
  })
</script>

{#if isMobileShell()}<MobileBotsPage />{/if}
