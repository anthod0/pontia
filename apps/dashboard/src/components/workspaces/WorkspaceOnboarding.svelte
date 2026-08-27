<script lang="ts">
  import { FolderPlus } from '@lucide/svelte'
  import WorkspaceBrowser from './WorkspaceBrowser.svelte'
</script>

<section class="mx-auto flex min-h-[calc(100svh-5.5rem)] max-w-5xl flex-col justify-center gap-6 md:min-h-[calc(100svh-6.5rem)]" aria-labelledby="workspace-onboarding-title">
  <div class="flex items-start gap-3 px-1">
    <div class="flex size-10 shrink-0 items-center justify-center rounded-lg bg-muted">
      <FolderPlus class="size-5" aria-hidden="true" />
    </div>
    <div class="space-y-1">
      <h1 id="workspace-onboarding-title" class="text-xl font-medium leading-snug">Set up your first workspace</h1>
      <p class="text-sm text-muted-foreground">A workspace is the project directory where Pontia starts and controls your agent sessions. Activate a directory below to continue.</p>
    </div>
  </div>

  <WorkspaceBrowser />
</section>
