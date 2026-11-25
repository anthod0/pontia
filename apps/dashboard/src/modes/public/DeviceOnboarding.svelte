<script lang="ts">
  import { t } from '$lib/i18n';
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { copyText } from '$lib/copyText'

  let { onCheck, checking }: { onCheck: () => Promise<boolean>; checking: boolean } = $props()

  const steps = $derived([
    {
      title: $t('devices.install'),
      description: $t('devices.installDescription'),
      command: 'npx @pontia/cli init',
    },
    {
      title: $t('devices.initialize'),
      description: $t('devices.initializeDescription'),
    },
    {
      title: $t('devices.openHere'),
      description: $t('devices.openDescription'),
    },
  ])
  let copied = $state<number | null>(null)

  let showEmptyHint = $state(false)

  async function checkDevices(): Promise<void> {
    showEmptyHint = false
    const foundDevices = await onCheck()
    showEmptyHint = !foundDevices
  }

  async function copy(index: number, command: string): Promise<void> {
    if (!(await copyText(command))) return
    copied = index
    setTimeout(() => { if (copied === index) copied = null }, 2000)
  }
</script>

<section class="flex min-h-[calc(100svh-3rem)] items-center justify-center p-4 sm:p-8" aria-labelledby="onboarding-title">
  <div class="w-full max-w-xl">
    <header class="mb-8 space-y-2">
      <h1 id="onboarding-title" class="text-2xl font-medium tracking-tight text-heading">{$t('devices.connectFirst')}</h1>
      <p class="text-sm leading-relaxed text-muted-foreground">
        {$t('devices.onboardingDescription')}
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
                  aria-label={$t('common.copyNamed', { value: step.command })} title={$t('common.copy')} onclick={() => void copy(index, step.command!)}>
                  {#if copied === index}<CheckIcon />{:else}<CopyIcon />{/if}
                </Button>
              </div>
            {:else if index === steps.length - 1}
              <Button variant="outline" size="sm" onclick={checkDevices} disabled={checking}>
                <ArrowClockwiseIcon class={checking ? 'animate-spin' : undefined} />
                {checking ? $t('common.checking') : $t('devices.check')}
              </Button>
              <div role="status" class="empty:hidden">
                {#if showEmptyHint}
                  <p class="text-xs text-muted-foreground motion-safe:animate-in motion-safe:fade-in-0 motion-safe:duration-300">{$t('devices.noneFound')}</p>
                {/if}
              </div>
            {/if}
          </div>
        </li>
      {/each}
    </ol>

    <footer class="mt-10 flex flex-wrap gap-x-5 gap-y-2 border-t border-border pt-5 text-xs text-muted-foreground">
      <a class="inline-flex items-center gap-1 hover:text-primary" href="https://pontia.dev/docs/getting-started" target="_blank" rel="noreferrer">
        {$t('devices.setupGuide')} <ArrowSquareOutIcon class="size-3" />
      </a>
      <a class="inline-flex items-center gap-1 hover:text-primary" href="https://pontia.dev/docs/troubleshooting" target="_blank" rel="noreferrer">
        {$t('devices.troubleshooting')} <ArrowSquareOutIcon class="size-3" />
      </a>
      <a class="inline-flex items-center gap-1 hover:text-primary" href="https://pontia.dev/settings/devices" target="_blank" rel="noreferrer">
        {$t('devices.manage')} <ArrowSquareOutIcon class="size-3" />
      </a>
    </footer>
  </div>
</section>
