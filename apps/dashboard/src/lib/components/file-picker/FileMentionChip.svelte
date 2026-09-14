<script lang="ts">
  import { File, Folder } from '@lucide/svelte'
  import * as Tooltip from '$lib/components/ui/tooltip/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'

  interface Props {
    path: string
    kind: string
  }

  let { path, kind }: Props = $props()
</script>

<Tooltip.Provider>
<Tooltip.Root>
  <Tooltip.Trigger>
    {#snippet child({ props })}
      <Badge
        {...props}
        variant="secondary"
        class="h-auto max-w-full cursor-default rounded-md px-1.5 py-0.5 align-baseline text-sm font-normal"
        aria-label={`${kind === 'directory' ? 'Directory' : 'File'} ${path}`}
      >
        {#if kind === 'directory'}
          <Folder class="size-3 shrink-0" aria-hidden="true" />
        {:else}
          <File class="size-3 shrink-0" aria-hidden="true" />
        {/if}
        <span class="truncate">@{path}</span>
      </Badge>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Content>{path}</Tooltip.Content>
</Tooltip.Root>
</Tooltip.Provider>
