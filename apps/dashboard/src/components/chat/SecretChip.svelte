<script lang="ts">
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon'
  import type { SecretFile } from '$lib/uploads/message'
  let {secret, failed = false}: {secret: Omit<SecretFile, 'path'>; failed?: boolean} = $props()
</script>
<div class={['flex size-full flex-col items-center justify-center gap-1', failed ? 'text-destructive' : 'text-muted-foreground']}>
  <KeyIcon class="size-6" />
  <span class={['max-w-full truncate px-1.5 text-10 font-semibold leading-none', !secret.label && 'uppercase tracking-wide']}>{secret.label ?? 'Key'}</span>
</div>
