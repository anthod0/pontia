<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import MobileProfilesSettings from '../views/mobile/settings/MobileProfilesSettings.svelte'
  import DesktopProfilesSettings from '../views/desktop/settings/DesktopProfilesSettings.svelte'
</script>

{#if isMobileShell()}<MobileProfilesSettings />{:else}<DesktopProfilesSettings />{/if}
