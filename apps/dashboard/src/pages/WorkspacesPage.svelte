<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import MobileWorkspacesSettings from '../views/mobile/settings/MobileWorkspacesSettings.svelte'
  import DesktopWorkspacesSettings from '../views/desktop/settings/DesktopWorkspacesSettings.svelte'

  let { openFirstAvailableRoot = false }: { openFirstAvailableRoot?: boolean } = $props()
</script>

{#if isMobileShell()}<MobileWorkspacesSettings {openFirstAvailableRoot} />{:else}<DesktopWorkspacesSettings {openFirstAvailableRoot} />{/if}
