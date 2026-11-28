<script lang="ts">
  import ResponsiveSelect from '$lib/components/ui/overlay/ResponsiveSelect.svelte'
  import { t, languages, languagePreference, type LanguagePreference } from '$lib/i18n';
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import MoonIcon from 'phosphor-svelte/lib/MoonIcon'
  import SunIcon from 'phosphor-svelte/lib/SunIcon'
  import * as ToggleGroup from '$lib/components/ui/toggle-group/index.js'
  import { themePreference, type ThemePreference } from '$lib/theme'

  $: options = [
    { value: 'system', label: $t('settings.system'), icon: DesktopIcon },
    { value: 'light', label: $t('settings.light'), icon: SunIcon },
    { value: 'dark', label: $t('settings.dark'), icon: MoonIcon },
  ]

  $: languageOptions = [
    { value: 'system', label: $t('settings.browserLanguage') },
    ...languages,
  ]

  function select(value: string): void {
    // A single toggle group emits an empty value when the active item is pressed again.
    if (value) themePreference.set(value as ThemePreference)
  }
</script>

<section class="space-y-4" aria-labelledby="settings-appearance-title">
  <h2 id="settings-appearance-title" class="text-sm font-semibold text-heading">{$t('settings.appearance')}</h2>
  <div class="flex flex-wrap items-center justify-between gap-4">
    <div class="min-w-0">
      <p class="text-sm font-medium text-heading">{$t('settings.theme')}</p>
      <p class="text-xs text-muted-foreground">{$t('settings.systemThemeHint')}</p>
    </div>
    <ToggleGroup.Root type="single" variant="outline" value={$themePreference} onValueChange={select} aria-label={$t('settings.theme')}>
      {#each options as option (option.value)}
        <ToggleGroup.Item value={option.value} class="px-3">
          <option.icon />
          {option.label}
        </ToggleGroup.Item>
      {/each}
    </ToggleGroup.Root>
  </div>
  <div class="flex flex-wrap items-center justify-between gap-4">
    <div class="min-w-0">
      <p class="text-sm font-medium text-heading">{$t('settings.language')}</p>
      <p class="text-xs text-muted-foreground">{$t('settings.languageHint')}</p>
    </div>
    <ResponsiveSelect value={$languagePreference} onValueChange={(value) => languagePreference.set(value as LanguagePreference)}
      label={$t('settings.language')} triggerClass="w-40"
      options={languageOptions} keyOf={language => language.value} labelOf={language => language.label}>
      {#snippet trigger()}
        {languageOptions.find((language) => language.value === $languagePreference)?.label}
      {/snippet}
      {#snippet option(language)}{language.label}{/snippet}
    </ResponsiveSelect>
  </div>
</section>
