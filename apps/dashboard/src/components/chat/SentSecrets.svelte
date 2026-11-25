<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements'
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon'
  import type { SecretFile } from '$lib/uploads/message'

  /** Secrets sent with a user message, as capsules with their labels and masked values. */
  let { secrets, class: className, ...rest }: HTMLAttributes<HTMLUListElement> & { secrets: SecretFile[] } = $props()
</script>

<ul class={['flex max-w-full flex-wrap justify-end gap-1.5', className]} data-user-message-secrets {...rest}>
  {#each secrets as secret (secret.path)}
    <li class="flex h-7 max-w-full min-w-0 items-center gap-1.5 rounded-full bg-muted px-2.5 text-xs">
      <KeyIcon class="size-3.5 shrink-0 text-muted-foreground" />
      {#if secret.label}<span class="max-w-48 shrink-0 truncate font-medium text-heading">{secret.label}</span>{/if}
      <code class="min-w-0 truncate font-mono text-muted-foreground">{secret.masked}</code>
    </li>
  {/each}
</ul>
