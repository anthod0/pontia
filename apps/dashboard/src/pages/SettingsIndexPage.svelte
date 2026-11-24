<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { navigate } from '$lib/navigation'
  import MobileDevicePage from '../views/mobile/MobileDevicePage.svelte'

  $effect(() => {
    if (!isMobileShell()) void navigate('/settings/common', undefined, { replaceState: true })
  })
</script>

{#if isMobileShell()}<MobileDevicePage />{/if}
