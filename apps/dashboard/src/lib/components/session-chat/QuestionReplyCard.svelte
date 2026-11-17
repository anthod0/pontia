<script lang="ts">
  import { t } from '$lib/i18n';
  import ChatCircleTextIcon from 'phosphor-svelte/lib/ChatCircleTextIcon'
  import type { QuestionReply } from '../../session-chat/questionReply'

  let { replies }: { replies: QuestionReply[] } = $props()
</script>

<div class="w-full space-y-3" data-question-reply>
  <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
    <ChatCircleTextIcon class="size-3.5" aria-hidden="true" />
    {$t('chat.questionReplyTitle', { count: replies.length })}
  </div>
  <ol class="space-y-3">
    {#each replies as reply, index (index)}
      <li class="space-y-1 border-l-2 border-border pl-3">
        <p class="whitespace-pre-wrap text-xs text-muted-foreground">{reply.question}</p>
        <p class="whitespace-pre-wrap">{reply.answer}</p>
      </li>
    {/each}
  </ol>
</div>
