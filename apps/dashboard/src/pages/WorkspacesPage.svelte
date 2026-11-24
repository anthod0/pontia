<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import MobileWorkspacesSettings from '../views/mobile/settings/MobileWorkspacesSettings.svelte'
  import DesktopWorkspacesSettings from '../views/desktop/settings/DesktopWorkspacesSettings.svelte'
</script>

{#if isMobileShell()}<MobileWorkspacesSettings />{:else}<DesktopWorkspacesSettings />{/if}
