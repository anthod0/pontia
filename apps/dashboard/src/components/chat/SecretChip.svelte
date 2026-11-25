<script lang="ts">
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon'
  import type { SecretFile } from '$lib/uploads/message'
  let {secret}: {secret: Omit<SecretFile, 'path'>} = $props()
</script>
<KeyIcon class="size-3.5 shrink-0"/>
{#if secret.label}<span class="max-w-64 truncate">{secret.label}</span>{/if}
<code class="break-all">{secret.masked}</code>
