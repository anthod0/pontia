<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { navigate } from '$lib/navigation'
  import ShortcutSettings from '../../../../components/settings/ShortcutSettings.svelte'

  import { useDashboardHost } from '../../../../device/hostContext'
  const host = useDashboardHost()
  $effect(() => {
    if (isMobileShell()) {
      const path = host.mode === 'public' && !host.routeScope ? '/' : '/settings'
      void navigate(path, undefined, { replaceState: true, scope: host.routeScope })
    }
  })
</script>

{#if !isMobileShell()}<ShortcutSettings />{/if}
