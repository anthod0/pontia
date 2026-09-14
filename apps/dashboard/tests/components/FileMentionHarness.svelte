<script lang="ts">
  import FileMentionEditor from '../../src/lib/components/file-picker/FileMentionEditor.svelte'

  interface Props {
    initialValue?: string
    workspaceId?: string
  }

  let { initialValue: value = $bindable(''), workspaceId = 'workspace-1' }: Props = $props()
</script>

<FileMentionEditor bind:value {workspaceId} placeholder="Message" />
<output data-testid="prompt-value">{value}</output>
<button type="button" onclick={() => (value = 'external @src/lib.rs\nnext line')}>Replace draft</button>
