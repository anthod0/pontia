<script lang="ts">
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import { Button } from '$lib/components/ui/button/index.js'
  import { type UnconfirmedSubmission } from "../../stores/inboxRecovery";
  const device = useDeviceRuntime();
  const { unconfirmedSubmissions } = device.inboxRecovery;
  const { recoverInboxSubmission } = device.sessions;

  let { sessionId }: { sessionId: string } = $props()
  let busy = $state<string | null>(null)
  let error = $state<string | null>(null)
  let submissions = $derived($unconfirmedSubmissions.filter((item) => item.sessionId === sessionId))

  async function recover(submission: UnconfirmedSubmission) {
    busy = submission.messageId
    error = null
    try { await recoverInboxSubmission(submission) }
    catch (failure) { error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = null }
  }
</script>

{#if submissions.length}
  <section class="mb-2 rounded-lg border bg-background p-3 text-sm" aria-label={$t('inbox.unconfirmed')}>
    <p class="font-medium">{$t('inbox.unknownReceipt')}</p>
    <p class="text-xs text-muted-foreground">{$t('inbox.recoverHint')}</p>
    {#each submissions as submission (submission.messageId)}
      <div class="mt-2 flex items-center gap-2">
        <span class="min-w-0 flex-1 truncate">{submission.input.input}</span>
        <Button variant="outline" size="sm" disabled={busy !== null} onclick={() => void recover(submission)}>{$t('inbox.recover')}</Button>
      </div>
    {/each}
    {#if error}<p role="alert" class="mt-2 text-destructive">{error}</p>{/if}
  </section>
{/if}
