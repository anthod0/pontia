<script lang="ts">
  import ModelEffortPicker from '../../src/components/chat/ModelEffortPicker.svelte'

  let { model = '', effort = '', allowDefault = false, onchange }: {
    model?: string
    effort?: string
    allowDefault?: boolean
    onchange?: (model: string, effort: string) => void
  } = $props()

  $effect(() => onchange?.(model, effort))
</script>

<ModelEffortPicker id="picker" client={{ name: 'Codex', client_type: 'codex', client_key: 'default', version: null }}
  bind:model bind:effort {allowDefault} variant={allowDefault ? 'toolbar' : 'form'} />
