<script lang="ts">
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import MoonIcon from 'phosphor-svelte/lib/MoonIcon'
  import SunIcon from 'phosphor-svelte/lib/SunIcon'
  import * as ToggleGroup from '$lib/components/ui/toggle-group/index.js'
  import { themePreference, type ThemePreference } from '$lib/theme'

  const options: { value: ThemePreference; label: string; icon: typeof SunIcon }[] = [
    { value: 'system', label: 'System', icon: DesktopIcon },
    { value: 'light', label: 'Light', icon: SunIcon },
    { value: 'dark', label: 'Dark', icon: MoonIcon },
  ]

  function select(value: string): void {
    // A single toggle group emits an empty value when the active item is pressed again.
    if (value) themePreference.set(value as ThemePreference)
  }
</script>

<section class="space-y-4" aria-labelledby="settings-appearance-title">
  <h2 id="settings-appearance-title" class="text-sm font-semibold text-heading">Appearance</h2>
  <div class="flex flex-wrap items-center justify-between gap-4">
    <div class="min-w-0">
      <p class="text-sm font-medium text-heading">Theme</p>
      <p class="text-xs text-muted-foreground">System follows your device setting.</p>
    </div>
    <ToggleGroup.Root type="single" variant="outline" value={$themePreference} onValueChange={select} aria-label="Theme">
      {#each options as option (option.value)}
        <ToggleGroup.Item value={option.value} class="px-3">
          <option.icon />
          {option.label}
        </ToggleGroup.Item>
      {/each}
    </ToggleGroup.Root>
  </div>
</section>
