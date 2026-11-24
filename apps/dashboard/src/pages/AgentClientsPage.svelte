<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import MobileClientsSettings from '../views/mobile/settings/MobileClientsSettings.svelte'
  import DesktopClientsSettings from '../views/desktop/settings/DesktopClientsSettings.svelte'
</script>

{#if isMobileShell()}<MobileClientsSettings />{:else}<DesktopClientsSettings />{/if}
