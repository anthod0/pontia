<script lang="ts">
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { copyText } from '$lib/copyText'

  let { onCheck, checking }: { onCheck: () => void; checking: boolean } = $props()

  const steps = [
    {
      title: 'Install Pontia on your device',
      description: 'Run the signed installer on the Linux or macOS machine where your coding agents run.',
      command: 'curl -fsSL https://get.pontia.dev/install.sh | sh',
    },
    {
      title: 'Initialize and register the device',
      description: 'Choose your agent clients, then accept the remote access prompt and approve the device in your browser.',
      command: '"$HOME/.local/bin/pontia" init',
    },
    {
      title: 'Open it here',
      description: 'When setup finishes, your device appears in this Dashboard and you can start a session.',
    },
  ]
  let copied = $state<number | null>(null)

  async function copy(index: number, command: string): Promise<void> {
    if (!(await copyText(command))) return
    copied = index
    setTimeout(() => { if (copied === index) copied = null }, 2000)
  }
</script>

<section class="flex min-h-[calc(100svh-3rem)] items-center justify-center p-4 sm:p-8" aria-labelledby="onboarding-title">
  <div class="w-full max-w-xl">
    <header class="mb-8 space-y-2">
      <h1 id="onboarding-title" class="text-2xl font-medium tracking-tight text-heading">Connect your first device</h1>
      <p class="text-sm leading-relaxed text-muted-foreground">
        Pontia runs agents on your own machine. Connect it once, then follow and steer sessions from any browser.
      </p>
    </header>

    <ol class="space-y-0">
      {#each steps as step, index (step.title)}
        <li class="relative flex gap-4 pb-8 last:pb-0">
          {#if index < steps.length - 1}
            <span aria-hidden="true" class="absolute top-8 bottom-0 left-[13px] w-px bg-border"></span>
          {/if}
          <span class="flex size-7 shrink-0 items-center justify-center rounded-full border border-primary/40 bg-primary/10 font-mono text-xs font-medium text-primary">
            {index + 1}
          </span>
          <div class="min-w-0 flex-1 space-y-2 pt-0.5">
            <h2 class="text-sm font-medium text-heading">{step.title}</h2>
            <p class="text-sm text-muted-foreground">{step.description}</p>
            {#if step.command}
              <div class="flex items-center gap-2 rounded-md bg-[#282828] py-1.5 pr-1.5 pl-3 text-[#ebdbb2]">
                <code class="min-w-0 flex-1 font-mono text-xs leading-relaxed break-all">
                  <span class="mr-2 text-[#928374] select-none">$</span>{step.command}
                </code>
                <Button variant="ghost" size="icon-sm" class="text-[#bdae93] hover:bg-[#3c3836] hover:text-[#ebdbb2]"
                  aria-label={`Copy ${step.command}`} title="Copy" onclick={() => void copy(index, step.command!)}>
                  {#if copied === index}<CheckIcon />{:else}<CopyIcon />{/if}
                </Button>
              </div>
            {:else}
              <Button variant="outline" size="sm" onclick={onCheck} disabled={checking}>
                <ArrowClockwiseIcon class={checking ? 'animate-spin' : undefined} />
                {checking ? 'Checking…' : 'Check for devices'}
              </Button>
            {/if}
          </div>
        </li>
      {/each}
    </ol>

    <footer class="mt-10 flex flex-wrap gap-x-5 gap-y-2 border-t border-border pt-5 text-xs text-muted-foreground">
      <a class="inline-flex items-center gap-1 hover:text-primary" href="https://pontia.dev/docs/getting-started" target="_blank" rel="noreferrer">
        Setup guide <ArrowSquareOutIcon class="size-3" />
      </a>
      <a class="inline-flex items-center gap-1 hover:text-primary" href="https://pontia.dev/docs/troubleshooting" target="_blank" rel="noreferrer">
        Troubleshooting <ArrowSquareOutIcon class="size-3" />
      </a>
      <a class="inline-flex items-center gap-1 hover:text-primary" href="https://pontia.dev/settings/devices" target="_blank" rel="noreferrer">
        Manage devices <ArrowSquareOutIcon class="size-3" />
      </a>
    </footer>
  </div>
</section>
