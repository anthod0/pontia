<script lang="ts">
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon'
  import type { SecretFile } from '$lib/uploads/message'
  let {secret}: {secret: Omit<SecretFile, 'path'>} = $props()
</script>
<KeyIcon class="size-3.5 shrink-0"/>
{#if secret.label}<span class="min-w-0 max-w-64 truncate">{secret.label}</span>{/if}
<code class="min-w-0 truncate rounded-md bg-foreground/5 px-1.5 py-0.5">{secret.masked}</code>
