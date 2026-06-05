<script lang="ts">
  import { KeyRound } from '@lucide/svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { token } from '../../stores/auth'

  let draftToken = ''
  let error = ''

  function continueToDashboard(): void {
    const trimmed = draftToken.trim()
    if (!trimmed) {
      error = 'Enter a token to continue.'
      return
    }
    token.set(trimmed)
  }
</script>

<main class="flex min-h-svh items-center justify-center bg-muted/20 p-4">
  <Card.Root class="w-full max-w-md">
    <Card.Header class="space-y-3 text-center">
      <div class="mx-auto flex size-12 items-center justify-center rounded-full bg-primary/10 text-primary">
        <KeyRound class="size-6" />
      </div>
      <div class="space-y-1">
        <h1 class="text-2xl font-semibold">Enter External API token</h1>
        <Card.Description>Access to /dashboard requires a bearer token.</Card.Description>
      </div>
    </Card.Header>
    <Card.Content>
      <form class="space-y-4" onsubmit={(event) => { event.preventDefault(); continueToDashboard() }}>
        <div class="space-y-2">
          <Label for="dashboard-auth-token">Bearer token</Label>
          <Input
            id="dashboard-auth-token"
            type="password"
            placeholder="Paste External API token"
            bind:value={draftToken}
            autocomplete="off"
          />
          <p class="text-xs text-muted-foreground">The token is stored only in this browser's localStorage.</p>
        </div>
        {#if error}<p class="text-sm text-destructive">{error}</p>{/if}
        <Button type="submit" class="w-full">Continue</Button>
      </form>
    </Card.Content>
  </Card.Root>
</main>
