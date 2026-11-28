<script lang="ts">
  import CommonSettings from '$dashboard-mode/SettingsCommonPage.svelte'
  import { t, languages, languagePreference, type LanguagePreference } from '$lib/i18n'
  import { themePreference, type ThemePreference } from '$lib/theme'
  import * as ToggleGroup from '$lib/components/ui/toggle-group'
</script>

<div class="mobile-common">
  <CommonSettings>
    {#snippet appearance()}
      <section class="grid gap-3" aria-label={$t('settings.appearance')}>
        <h2 class="font-semibold">{$t('settings.appearance')}</h2>
        <p>{$t('settings.theme')}</p>
        <ToggleGroup.Root
          type="single"
          variant="outline"
          value={$themePreference}
          aria-label={$t('settings.theme')}
          onValueChange={(value) => {
            if (value) themePreference.set(value as ThemePreference)
          }}
        >
          {#each ['system', 'light', 'dark'] as const as theme (theme)}
            <ToggleGroup.Item value={theme} class="flex-1">{$t(`settings.${theme}`)}</ToggleGroup.Item>
          {/each}
        </ToggleGroup.Root>
        <p class="text-xs text-muted-foreground">{$t('settings.systemThemeHint')}</p>
        <p>{$t('settings.language')}</p>
        <ToggleGroup.Root
          type="single"
          variant="outline"
          class="flex-wrap"
          value={$languagePreference}
          aria-label={$t('settings.language')}
          onValueChange={(value) => {
            if (value) languagePreference.set(value as LanguagePreference)
          }}
        >
          <ToggleGroup.Item value="system" class="flex-1">{$t('settings.browserLanguage')}</ToggleGroup.Item>
          {#each languages as language (language.value)}
            <ToggleGroup.Item value={language.value} class="flex-1">{language.label}</ToggleGroup.Item>
          {/each}
        </ToggleGroup.Root>
        <p class="text-xs text-muted-foreground">{$t('settings.languageHint')}</p>
      </section>
    {/snippet}
  </CommonSettings>
</div>

<style>
  .mobile-common :global([data-slot='card']) {
    gap: 0.75rem;
    padding-block: 1rem;
  }
  .mobile-common :global(button),
  .mobile-common :global(a) {
    min-height: 44px;
  }
</style>
