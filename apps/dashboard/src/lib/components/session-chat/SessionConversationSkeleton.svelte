<script lang="ts">
  import { t } from '$lib/i18n';
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'

  let { class: className = '' }: { class?: string } = $props()
</script>

<div data-chat-conversation-skeleton class={`space-y-8 py-4 ${className}`} role="status" aria-label={$t('chat.loadingConversation')}>
  <div class="flex justify-end"><Skeleton class="h-14 w-3/5 max-w-xl rounded-item" /></div>
  <div class="w-4/5 max-w-2xl space-y-3">
    <Skeleton class="h-4 w-full" />
    <Skeleton class="h-4 w-11/12" />
    <Skeleton class="h-4 w-2/3" />
  </div>
  <div class="flex justify-end"><Skeleton class="h-10 w-2/5 max-w-md rounded-item" /></div>
  <div class="w-3/4 max-w-xl space-y-3">
    <Skeleton class="h-4 w-full" />
    <Skeleton class="h-4 w-4/5" />
  </div>
</div>
