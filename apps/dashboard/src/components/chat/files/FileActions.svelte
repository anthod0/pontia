<script lang="ts">
  import type { Snippet } from 'svelte'
  import FileAction from './FileAction.svelte'
  import type { FileActionLook } from './fileActionButton'
  import type { SharedFileState } from './sharedFileState.svelte'

  /** A file row's download action followed by the caller's actions, kept close together. */
  let { itemActions, state }: { itemActions?: Snippet<[FileActionLook]>; state: SharedFileState } = $props()
</script>

<div class="flex shrink-0 items-center gap-0.5">
  <FileAction {state} />
  {@render itemActions?.('row')}
</div>
