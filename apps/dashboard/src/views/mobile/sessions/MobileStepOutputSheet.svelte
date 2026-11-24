<script lang="ts">
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { t } from '$lib/i18n'
  import { managedToolUseTitle, type SessionChatThoughtStep } from '$lib/session-chat/sessionChat'
  import ThoughtExecution from '$lib/components/session-chat/ThoughtExecution.svelte'
  let { step, close }: { step: SessionChatThoughtStep; close: () => void } = $props()
</script>

<Sheet.Root
  open
  onOpenChange={(open) => {
    if (!open) close()
  }}
>
  <BottomSheet dismiss={close} aria-describedby={undefined}>
    <Sheet.Title class="truncate py-3 pr-10 font-semibold"
      >{step.managedToolUse ? managedToolUseTitle(step.managedToolUse, $t) : step.title}</Sheet.Title
    >
    <div class="overflow-auto rounded-xl bg-muted p-3 text-xs">
      <pre class="whitespace-pre-wrap break-words">{step.content}</pre>
      {#if step.executionStatus}<ThoughtExecution {step} />{:else if step.result}<pre
          class="mt-3 whitespace-pre-wrap break-words">{step.result.content}</pre>{/if}
    </div></BottomSheet
  ></Sheet.Root
>
