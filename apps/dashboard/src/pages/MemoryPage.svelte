<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import MobileMemorySettings from '../views/mobile/settings/MobileMemorySettings.svelte'
  import DesktopMemorySettings from '../views/desktop/settings/DesktopMemorySettings.svelte'
</script>

{#if isMobileShell()}<MobileMemorySettings />{:else}<DesktopMemorySettings />{/if}
