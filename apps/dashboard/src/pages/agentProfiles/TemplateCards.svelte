<script lang="ts">
  import { t, currentLocale } from '$lib/i18n'
  import { cn } from '$lib/utils.js'
  import type { AgentClientView } from '../../api/types'
  import ClientIcon from '../../components/chat/ClientIcon.svelte'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { profileTemplates, templateClientNames, templateClientStatus, type ProfileTemplate } from './templates'

  let { clients, selected, onchoose, class: className }: {
    clients: AgentClientView[]
    selected?: string
    onchoose: (template: ProfileTemplate) => void
    class?: string
  } = $props()
</script>

<div class={cn('no-scrollbar -mx-4 flex snap-x snap-mandatory scroll-px-4 gap-2 overflow-x-auto px-4 sm:mx-0 sm:grid sm:grid-cols-2 sm:overflow-visible sm:px-0', className)}>
  {#each profileTemplates as template (template.id)}
    {@const text = template.text[$currentLocale]}
    {@const status = templateClientStatus(template, clients)}
    {@const clientName = templateClientNames[template.client_type]}
    <button type="button" aria-pressed={selected === template.id} onclick={() => onchoose(template)}
      class="group/card flex w-64 shrink-0 snap-start flex-col gap-2 rounded-lg border bg-card p-3 text-left transition-colors outline-none hover:border-primary/60 hover:bg-muted/40 focus-visible:ring-3 focus-visible:ring-ring/50 aria-pressed:border-primary aria-pressed:bg-selected sm:w-auto sm:min-w-0">
      <span class="flex min-w-0 items-center gap-2.5">
        <span class="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-foreground group-aria-pressed/card:bg-background"><template.icon class="size-4" /></span>
        <span class="min-w-0 flex-1 truncate text-sm font-medium text-heading">{text.name}</span>
        {#if template.use_yolo}<span class="inline-flex h-5 shrink-0 items-center rounded-md border px-1.5 text-[0.7rem] font-medium text-muted-foreground">YOLO</span>{/if}
        {#if selected === template.id}<CheckIcon class="size-4 shrink-0 text-primary" />{/if}
      </span>
      <span class="line-clamp-2 min-h-8 text-xs leading-4 text-muted-foreground">{text.description}</span>
      <span class="flex min-w-0 items-center gap-1.5 text-xs {status === 'available' ? 'text-foreground' : 'text-muted-foreground'}"
        title={status === 'available' ? undefined : $t(status === 'missing' ? 'profiles.templateClientMissingTitle' : 'profiles.templateClientDisabledTitle', { client: clientName })}>
        <ClientIcon clientType={template.client_type} class="size-3.5 shrink-0 {status === 'available' ? 'text-foreground' : 'opacity-50 grayscale'}" />
        <span class="shrink-0">{clientName}</span>
        {#if status === 'available'}
          <span class="min-w-0 truncate text-muted-foreground">· <span class="font-mono">{template.model}</span></span>
        {:else}
          <span class="inline-flex min-w-0 items-center gap-1 truncate">· <WarningCircleIcon class="size-3.5 shrink-0" />{$t(status === 'missing' ? 'profiles.templateClientMissingShort' : 'profiles.templateClientDisabledShort')}</span>
        {/if}
      </span>
    </button>
  {/each}
</div>
