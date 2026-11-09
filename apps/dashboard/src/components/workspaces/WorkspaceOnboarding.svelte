<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import PontiaMascot from '../chat/PontiaMascot.svelte'
  import WorkspaceBrowser from './WorkspaceBrowser.svelte'

  interface Props {
    canContinue: boolean
    onContinue: () => void
  }

  let { canContinue, onContinue }: Props = $props()
</script>

<section class="mx-auto flex min-h-[calc(100svh-5.5rem)] max-w-5xl flex-col justify-center gap-6 md:min-h-[calc(100svh-6.5rem)]" aria-labelledby="workspace-onboarding-title">
  <div class="flex items-center gap-3 px-1">
    <PontiaMascot class="size-12 shrink-0" />
    <h1 id="workspace-onboarding-title" class="text-xl font-medium leading-snug">Set up your first workspace</h1>
  </div>

  <WorkspaceBrowser />

  <div class="flex justify-end px-1">
    <Button disabled={!canContinue} onclick={onContinue}>Continue to New Chat</Button>
  </div>
</section>
