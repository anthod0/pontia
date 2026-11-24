<script lang="ts">
	import { onMount } from 'svelte';
	import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';
	import PackageIcon from 'phosphor-svelte/lib/PackageIcon';
	import DotsThreeVerticalIcon from 'phosphor-svelte/lib/DotsThreeVerticalIcon';
	import FileMagnifyingGlassIcon from 'phosphor-svelte/lib/FileMagnifyingGlassIcon';
	import FilePlusIcon from 'phosphor-svelte/lib/FilePlusIcon';
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import FolderIcon from 'phosphor-svelte/lib/FolderIcon';
	import GaugeIcon from 'phosphor-svelte/lib/GaugeIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import AtIcon from 'phosphor-svelte/lib/AtIcon';
	import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon';
	import CircleHalfIcon from 'phosphor-svelte/lib/CircleHalfIcon';
	import LightbulbIcon from 'phosphor-svelte/lib/LightbulbIcon';
	import PlusIcon from 'phosphor-svelte/lib/PlusIcon';
	import StopIcon from 'phosphor-svelte/lib/StopIcon';
	import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon';
	import { MediaQuery } from 'svelte/reactivity';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Popover from '$lib/components/ui/popover/index.js';
	import { cn } from '$lib/utils';
	import DemoInline from './DemoInline.svelte';
	import { demoClients, type DemoSession, type DemoStep, type DemoWorkspace } from './mockData';

	let { session, workspace, onopenfile }: { session: DemoSession; workspace: DemoWorkspace; onopenfile: (path: string) => void } = $props();

	let scroller: HTMLDivElement;
	onMount(() => {
		scroller.scrollTop = scroller.scrollHeight;
	});

	const stepIcons = { read: FileMagnifyingGlassIcon, edit: FileTextIcon, write: FilePlusIcon, command: TerminalWindowIcon, thinking: LightbulbIcon };
	const working = $derived(session.state === 'busy');
	const client = $derived(demoClients[session.client]);
	const git = $derived(workspace.git);
	const waiting = $derived(session.state === 'waiting_for_interaction');
	const model = $derived([session.model, session.reasoningEffort].filter(Boolean).join(' · '));
	const summary = $derived([client.name, workspace.name, git.branch, model, session.context, session.handle].filter(Boolean).join(' · '));
	let stepsOpen = $state(false);
	let root = $state<HTMLElement>();
	const phone = new MediaQuery('(max-width: 767px)');
	const sheet = $derived(phone.current && root);
	const Root = $derived(sheet ? Dialog.Root : Popover.Root);
	const Trigger = $derived(sheet ? Dialog.Trigger : Popover.Trigger);

	function stepLabel(step: DemoStep): string {
		if (step.kind === 'command') return step.title;
		if (step.kind === 'thinking') return step.text;
		const verb = step.kind === 'read' ? 'Read' : step.kind === 'edit' ? 'Edit' : 'Write';
		return `${verb} ${step.files} ${step.files === 1 ? 'file' : 'files'}`;
	}

	function formatDuration(totalSeconds: number): string {
		const minutes = Math.floor(totalSeconds / 60);
		const seconds = totalSeconds % 60;
		return minutes ? `${minutes}m ${seconds}s` : `${seconds}s`;
	}

</script>

{#snippet details()}
	<dl class="space-y-3 text-sm">
		{#snippet row(label: string, Icon: typeof FolderIcon | null, value: string)}
			<div class="grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2">
				<dt class="flex items-start justify-center text-muted-foreground">
					{#if Icon}<Icon class="size-4" aria-label={label} />{:else}{@render clientIcon('size-4')}<span class="sr-only">{label}</span>{/if}
				</dt>
				<dd class="min-w-0 font-mono text-xs break-words">{value}</dd>
			</div>
		{/snippet}
		{@render row('Client', null, client.name)}
		{@render row('Workspace', FolderIcon, workspace.path)}
		<div class="grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2">
			<dt class="flex items-start justify-center text-muted-foreground"><GitBranchIcon class="size-4" aria-label="Git" /></dt>
			<dd class="flex min-w-0 flex-wrap gap-1 font-mono text-xs">{@render gitStatus()}</dd>
		</div>
		{@render row('Model', PackageIcon, model)}
		{@render row('Context', GaugeIcon, session.context)}
		{@render row('Handle', AtIcon, session.handle)}
	</dl>
{/snippet}

{#snippet clientIcon(size: string)}
	<img class={cn(size, 'shrink-0 object-contain', session.client === 'codex' && 'dark:hidden')} src={`/client-logos/${client.logo}.svg`} alt="" />
	{#if session.client === 'codex'}<img class={cn(size, 'hidden shrink-0 object-contain dark:block')} src="/client-logos/codex-dark.svg" alt="" />{/if}
{/snippet}

{#snippet gitStatus()}
	<span>{git.branch}</span>
	{#if git.ahead}<span class="text-primary">↑{git.ahead}</span>{/if}
	{#if git.behind}<span class="text-(color:--demo-git-behind)">↓{git.behind}</span>{/if}
	{#each [['staged', '+', 'text-(color:--demo-git-added)'], ['unstaged', '~', 'text-(color:--demo-git-modified)'], ['untracked', '?', 'text-(color:--demo-git-untracked)']] as [group, symbol, tone] (group)}
		{@const count = git.changes.filter((change) => change.group === group).length}
		{#if count}<span class={tone}>{symbol}{count}</span>{/if}
	{/each}
{/snippet}

<div bind:this={root} class="relative flex min-h-0 flex-1 transform-gpu flex-col has-[[data-dialog-content]]:z-20">
	<div bind:this={scroller} class="demo-scroll min-h-0 flex-1 overflow-y-auto px-4 md:px-8">
		<div class="mx-auto flex w-full max-w-[760px] flex-col gap-4 pt-14 pb-6 @3xl:pt-12">
			<div class="flex flex-col items-end gap-1.5">
				<div class="max-w-[min(95%,720px)] rounded-lg border border-border bg-background px-4 py-3 text-[13.5px] leading-relaxed text-heading">
					<DemoInline onlink={onopenfile} text={session.prompt} />
				</div>
				<div class="flex h-7 items-center gap-0.5 text-muted-foreground" aria-hidden="true">
					<span class="px-1 text-xs tabular-nums">{session.steps.length ? '10:42 AM' : 'Just now'}</span>
					<span class="flex size-7 items-center justify-center"><CopyIcon class="size-3.5" /></span>
				</div>
			</div>

			{#if session.steps.length || session.reply.length}
			<div class="min-w-0 text-[13.5px] leading-relaxed">
				<div class="mb-3">
					<button
						type="button"
						class="inline-flex max-w-full min-w-0 items-center gap-1.5 rounded-sm py-1 pr-3 text-sm leading-5 text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary"
						aria-expanded={stepsOpen}
						aria-label={stepsOpen ? 'Hide agent work steps' : 'Show agent work steps'}
						onclick={() => (stepsOpen = !stepsOpen)}
					>
						<span class="truncate">{session.steps.length} {session.steps.length === 1 ? 'step' : 'steps'}</span>
						<CaretRightIcon class={cn('size-4 shrink-0 transition-transform', stepsOpen && 'rotate-90')} aria-hidden="true" />
					</button>
					{#if stepsOpen}
						<ul class="space-y-1 pt-1 pb-2">
							{#each session.steps as step, index}
								{@const Icon = stepIcons[step.kind]}
								<li class="relative flex gap-3 py-1.5">
									<span class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-surface text-muted-foreground">
										<Icon class="size-4" aria-hidden="true" />
									</span>
									<span class="pt-0.5 text-sm leading-5 font-medium text-foreground/75">{stepLabel(step)}</span>
									{#if index < session.steps.length - 1}
										<span class="absolute top-[1.875rem] -bottom-[0.625rem] left-[0.71875rem] w-px bg-border" aria-hidden="true"></span>
									{/if}
								</li>
							{/each}
						</ul>
					{/if}
				</div>

				<div class="text-sm leading-[1.85] text-foreground [&>*:first-child]:mt-0">
					{#each session.reply as block}
						{#if block.kind === 'p'}
							<div class="my-3"><DemoInline onlink={onopenfile} text={block.text} /></div>
						{:else if block.kind === 'ul'}
							<ul class="my-3 list-disc pl-5">
								{#each block.items as item}<li class="my-1"><DemoInline onlink={onopenfile} text={item} /></li>{/each}
							</ul>
						{:else}
							<div class="my-4 overflow-hidden rounded-lg border border-border bg-muted">
								<div class="flex items-center justify-between gap-3 px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
									<span class="font-medium tracking-wide uppercase">{block.language}</span>
									<span class="inline-flex items-center gap-1.5 px-2 py-1 font-sans text-xs font-medium" aria-hidden="true"><CopyIcon class="size-3.5" />Copy</span>
								</div>
								<pre class="overflow-x-auto px-4 pt-1 pb-4 font-mono text-xs leading-relaxed text-foreground"><code>{block.code}</code></pre>
							</div>
						{/if}
					{/each}
				</div>

				<div class="mt-2 flex h-7 items-center gap-x-1 text-muted-foreground">
					{#if waiting}
						<span class="flex items-center gap-2 pr-3" role="status">
							<CircleHalfIcon class="size-4" aria-hidden="true" />
							<span class="text-xs">Waiting for interaction</span>
						</span>
					{:else}
						<span class="px-1 text-xs">{working ? 'working for' : 'worked for'} {formatDuration(session.duration)}</span>
					{/if}
					{#if !working}
						<span class="px-1 text-xs tabular-nums" aria-hidden="true">10:44 AM</span>
						<span class="flex size-7 items-center justify-center" aria-hidden="true"><CopyIcon class="size-3.5" /></span>
					{/if}
				</div>
			</div>

			{/if}
			{#if session.state === 'exited'}
				<div class="flex justify-start text-xs text-muted-foreground" role="status">session exited · send a message to resume</div>
			{/if}
		</div>
	</div>

	<div class="relative bg-surface px-4 pb-2 md:px-8 md:pb-5">
		<div class="pointer-events-none absolute inset-x-0 bottom-full h-6 bg-linear-to-t from-surface to-surface/0"></div>
		<div class="mx-auto w-full max-w-[760px]">
			<div class="mb-1 flex min-w-0 items-center justify-end gap-2">
				<div class="min-w-0 flex-1">
				<Root>
					<Trigger
						class="flex h-7 max-md:min-h-11 w-fit max-w-full min-w-0 items-center rounded-md px-1.5 text-left font-mono text-[11px] text-muted-foreground outline-hidden hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary"
						aria-label={`Session details: ${summary}`}
					>
						<span class="block min-w-0 truncate">
							<span class="inline-flex items-center gap-1 align-middle">{@render clientIcon('size-[13px]')}<span>{client.name}</span></span>
							<span class="mx-1.5 inline-block h-2.5 border-l border-border align-middle"></span>
							<span class="inline-flex items-center gap-1 align-middle"><FolderIcon class="size-[13px] shrink-0" /><span>{workspace.name}</span></span>
							<span class="mx-1.5 inline-block h-2.5 border-l border-border align-middle"></span>
							<span class="inline-flex items-center gap-1 align-middle"><GitBranchIcon class="size-[13px] shrink-0" />{@render gitStatus()}</span>
							<span class="mx-1.5 inline-block h-2.5 border-l border-border align-middle"></span>
							<span class="inline-flex items-center gap-1 align-middle"><PackageIcon class="size-[13px]" />{model}</span>
							<span class="mx-1.5 inline-block h-2.5 border-l border-border align-middle"></span>
							<span class="align-middle">{session.context}</span>
							<span class="mx-1.5 inline-block h-2.5 border-l border-border align-middle"></span>
							<span class="inline-flex items-center gap-1 align-middle"><AtIcon class="size-[13px] shrink-0" /><span>{session.handle}</span></span>
						</span>
					</Trigger>
					{#if sheet}
						<Dialog.Content portalProps={{ to: sheet }} class="demo-git-colors inset-x-0 top-auto bottom-0 left-0 max-h-full max-w-none translate-x-0 translate-y-0 overflow-auto rounded-t-xl rounded-b-none p-4">
							<Dialog.Title>Session details</Dialog.Title>
							{@render details()}
						</Dialog.Content>
					{:else}
						<Popover.Content side="top" align="start" collisionBoundary={root} collisionPadding={8} aria-label="Session details" class="demo-git-colors w-96 max-w-(--bits-floating-available-width) rounded-lg p-3">
							{@render details()}
						</Popover.Content>
					{/if}
				</Root>
				</div>
				<span class="flex size-7 shrink-0 items-center justify-center text-foreground" aria-hidden="true"><DotsThreeVerticalIcon class="size-5" /></span>
			</div>
			<div class="flex items-center rounded-lg border border-border bg-background" aria-hidden="true">
				<span class="flex size-7 shrink-0 items-center justify-center pl-1 text-foreground"><PlusIcon class="size-4" /></span>
				<div class="min-w-0 flex-1 py-2 pl-1.5 text-[13.5px] text-muted-foreground">Follow up…</div>
				<div class="shrink-0 p-1">
					{#if working}
						<span class="inline-flex size-7 items-center justify-center rounded-md bg-primary-solid text-primary-foreground"><StopIcon class="size-4" /></span>
					{:else}
						<span class="inline-flex h-7 w-7 items-center justify-center gap-1.5 rounded-md bg-border text-xs font-semibold text-inactive @md:w-auto @md:px-3">
							<span class="hidden @md:inline">Send</span><ArrowUpIcon class="size-3.5" />
						</span>
					{/if}
				</div>
			</div>
		</div>
	</div>
</div>
