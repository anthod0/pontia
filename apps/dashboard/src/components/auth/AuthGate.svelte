<script lang="ts">
  import { t } from '$lib/i18n';
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { useDeviceRuntime } from '../../device/context'
  const { validateExternalApiToken } = useDeviceRuntime().api
  import { token } from '$dashboard-mode/auth'

  let draftToken = ''
  let error = ''
  let validating = false

  async function continueToDashboard(): Promise<void> {
    const trimmed = draftToken.trim()
    if (!trimmed) {
      error = $t('auth.tokenRequired')
      return
    }

    validating = true
    error = ''
    try {
      await validateExternalApiToken(trimmed)
      token.set(trimmed)
    } catch {
      error = $t('auth.invalidToken')
    } finally {
      validating = false
    }
  }
</script>

<main class="flex min-h-svh items-center justify-center bg-muted/20 p-4">
  <Card.Root class="w-full max-w-md">
    <Card.Header class="space-y-3 text-center">
      <div class="mx-auto flex size-12 items-center justify-center rounded-item bg-primary/10 text-primary">
        <KeyIcon class="size-6" />
      </div>
      <div class="space-y-1">
        <h1 class="text-2xl font-semibold">{$t('auth.enterToken')}</h1>
        <Card.Description>{$t('auth.accessDescription')}</Card.Description>
      </div>
    </Card.Header>
    <Card.Content>
      <form class="space-y-4" onsubmit={(event) => { event.preventDefault(); continueToDashboard() }}>
        <div class="space-y-2">
          <Label for="dashboard-auth-token">{$t('auth.bearerToken')}</Label>
          <Input
            id="dashboard-auth-token"
            type="password"
            placeholder={$t('auth.pasteToken')}
            bind:value={draftToken}
            autocomplete="off"
          />
          <p class="text-xs text-muted-foreground">{$t('auth.localStorageDescription')}</p>
        </div>
        {#if error}<p class="text-sm text-destructive">{error}</p>{/if}
        <Button type="submit" class="w-full" disabled={validating}>{validating ? $t('auth.validating') : $t('common.continue')}</Button>
      </form>
    </Card.Content>
  </Card.Root>
</main>
