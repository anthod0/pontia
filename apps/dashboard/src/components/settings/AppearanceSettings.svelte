<script lang="ts">
  import { t, languagePreference, type LanguagePreference } from '$lib/i18n';
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import MoonIcon from 'phosphor-svelte/lib/MoonIcon'
  import SunIcon from 'phosphor-svelte/lib/SunIcon'
  import * as ToggleGroup from '$lib/components/ui/toggle-group/index.js'
  import { themePreference, type ThemePreference } from '$lib/theme'

  import * as Select from '$lib/components/ui/select/index.js'

  $: options = [
    { value: 'system', label: $t('settings.system'), icon: DesktopIcon },
    { value: 'light', label: $t('settings.light'), icon: SunIcon },
    { value: 'dark', label: $t('settings.dark'), icon: MoonIcon },
  ]

  $: languages = [
    { value: 'system', label: $t('settings.browserLanguage') },
    { value: 'zh-CN', label: '简体中文' },
    { value: 'en', label: 'English' },
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
    <Select.Root type="single" value={$languagePreference} onValueChange={(value) => languagePreference.set(value as LanguagePreference)}>
      <Select.Trigger class="w-40" aria-label={$t('settings.language')}>
        {languages.find((language) => language.value === $languagePreference)?.label}
      </Select.Trigger>
      <Select.Content>
        {#each languages as language (language.value)}
          <Select.Item value={language.value} label={language.label}>{language.label}</Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </div>
</section>
