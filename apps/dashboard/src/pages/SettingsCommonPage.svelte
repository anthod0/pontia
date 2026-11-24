<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import CommonSettings from '$dashboard-mode/SettingsCommonPage.svelte'
  import MobileCommonSettings from '../views/mobile/settings/MobileCommonSettings.svelte'
</script>

{#if isMobileShell()}<MobileCommonSettings />{:else}<CommonSettings />{/if}
