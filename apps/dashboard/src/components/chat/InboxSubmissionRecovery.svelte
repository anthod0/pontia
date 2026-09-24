<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import { unconfirmedSubmissions, type UnconfirmedSubmission } from '../../stores/inboxRecovery'
  import { recoverInboxSubmission } from '../../stores/sessions'

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
  <section class="mb-2 border bg-background p-3 text-sm" aria-label="Unconfirmed submissions">
    <p class="font-medium">Submission receipt unknown</p>
    <p class="text-xs text-muted-foreground">The server may have accepted these inputs. Check or recover the same submission.</p>
    {#each submissions as submission (submission.messageId)}
      <div class="mt-2 flex items-center gap-2">
        <span class="min-w-0 flex-1 truncate">{submission.input.input}</span>
        <Button variant="outline" size="sm" disabled={busy !== null} onclick={() => void recover(submission)}>Check / recover submission</Button>
      </div>
    {/each}
    {#if error}<p role="alert" class="mt-2 text-destructive">{error}</p>{/if}
  </section>
{/if}
