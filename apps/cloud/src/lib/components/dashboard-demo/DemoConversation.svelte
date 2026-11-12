<script lang="ts">
	import { onMount } from 'svelte';
	import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';
	import CpuIcon from 'phosphor-svelte/lib/CpuIcon';
	import DotsThreeVerticalIcon from 'phosphor-svelte/lib/DotsThreeVerticalIcon';
	import FileMagnifyingGlassIcon from 'phosphor-svelte/lib/FileMagnifyingGlassIcon';
	import FilePlusIcon from 'phosphor-svelte/lib/FilePlusIcon';
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import FolderIcon from 'phosphor-svelte/lib/FolderIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import AtIcon from 'phosphor-svelte/lib/AtIcon';
	import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon';
	import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon';
	import type { DemoSession, DemoStep, DemoWorkspace } from './mockData';

	let { session, workspace }: { session: DemoSession; workspace: DemoWorkspace | undefined } = $props();

	let scroller: HTMLDivElement;
	onMount(() => {
		scroller.scrollTop = scroller.scrollHeight;
	});

	const stepIcons = { read: FileMagnifyingGlassIcon, edit: FileTextIcon, write: FilePlusIcon, command: TerminalWindowIcon };
	const working = $derived(session.state === 'busy');
	const metadata = $derived([
		{ icon: FolderIcon, value: workspace?.name ?? session.workspaceId },
		{ icon: GitBranchIcon, value: session.branch },
		{ icon: TerminalWindowIcon, value: session.client },
		{ icon: CpuIcon, value: session.model },
		{ icon: AtIcon, value: session.handle }
	]);

	function stepLabel(step: DemoStep): string {
		if (step.kind === 'command') return step.title;
		const verb = step.kind === 'read' ? 'Read' : step.kind === 'edit' ? 'Edit' : 'Write';
		return `${verb} ${step.files} ${step.files === 1 ? 'file' : 'files'}`;
	}

	function formatDuration(totalSeconds: number): string {
		const minutes = Math.floor(totalSeconds / 60);
		const seconds = totalSeconds % 60;
		return minutes ? `${minutes}m ${seconds}s` : `${seconds}s`;
	}

	// Mock replies only use **bold** and `code`.
	function inlineTokens(text: string): { kind: 'text' | 'strong' | 'code'; value: string }[] {
		return text
			.split(/(\*\*[^*]+\*\*|`[^`]+`)/)
			.filter(Boolean)
			.map((part) =>
				part.startsWith('**') ? { kind: 'strong', value: part.slice(2, -2) }
					: part.startsWith('`') ? { kind: 'code', value: part.slice(1, -1) }
						: { kind: 'text', value: part }
			);
	}
</script>

{#snippet inline(text: string)}
	{#each inlineTokens(text) as token}
		{#if token.kind === 'strong'}<strong class="font-semibold text-heading">{token.value}</strong>
		{:else if token.kind === 'code'}<code class="rounded-sm bg-muted px-1 py-0.5 font-mono text-xs text-primary">{token.value}</code>
		{:else}{token.value}{/if}
	{/each}
{/snippet}

<div class="flex min-h-0 flex-1 flex-col">
	<div bind:this={scroller} class="demo-scroll min-h-0 flex-1 overflow-y-auto px-4 md:px-8">
		<div class="mx-auto flex w-full max-w-[760px] flex-col gap-6 pt-4 pb-6">
			<div class="flex flex-col items-end gap-1.5">
				<div class="max-w-[min(95%,720px)] rounded-lg border border-border bg-background px-4 py-3 text-[13.5px] leading-relaxed text-heading">
					{@render inline(session.prompt)}
				</div>
				<div class="flex h-7 items-center gap-0.5 text-muted-foreground" aria-hidden="true">
					<span class="px-1 text-xs tabular-nums">10:42</span>
					<span class="flex size-7 items-center justify-center"><CopyIcon class="size-3.5" /></span>
				</div>
			</div>

			<div class="min-w-0 text-[13.5px] leading-relaxed">
				<div class="mb-3">
					<div class="inline-flex items-center gap-1.5 py-2.5 pr-3 text-sm leading-5 text-muted-foreground">
						<span>{working ? 'working for' : 'worked for'} {formatDuration(session.duration)}</span>
						<CaretRightIcon class="size-4 rotate-90" aria-hidden="true" />
					</div>
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
				</div>

				<div class="text-sm leading-[1.85] text-foreground [&>*:first-child]:mt-0">
					{#each session.reply as block}
						{#if block.kind === 'p'}
							<div class="my-3">{@render inline(block.text)}</div>
						{:else if block.kind === 'ul'}
							<ul class="my-3 list-disc pl-5">
								{#each block.items as item}<li class="my-1">{@render inline(item)}</li>{/each}
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

				{#if !working}
					<div class="mt-2 flex h-7 items-center text-muted-foreground" aria-hidden="true">
						<span class="px-1 text-xs tabular-nums">10:44</span>
						<span class="flex size-7 items-center justify-center"><CopyIcon class="size-3.5" /></span>
					</div>
				{/if}
			</div>

			{#if session.state === 'exited'}
				<div class="flex justify-start text-xs text-muted-foreground" role="status">session exited · send a message to resume</div>
			{/if}
		</div>
	</div>

	<div class="relative bg-surface px-4 pb-2 md:px-8 md:pb-5" aria-hidden="true">
		<div class="pointer-events-none absolute inset-x-0 bottom-full h-6 bg-linear-to-t from-surface to-surface/0"></div>
		<div class="mx-auto w-full max-w-[760px]">
			<div class="mb-1 flex min-w-0 items-center justify-end gap-2">
				<div class="min-w-0 flex-1 truncate px-1.5 font-mono text-[11px] text-muted-foreground">
					{#each metadata as field, index}
						{#if index > 0}<span class="mx-1.5 inline-block h-2.5 border-l border-border align-middle"></span>{/if}
						<span class="inline-flex items-center gap-1 align-middle">
							<field.icon class="size-[13px] shrink-0" />
							<span>{field.value}</span>
						</span>
					{/each}
				</div>
				<span class="flex size-7 shrink-0 items-center justify-center text-foreground"><DotsThreeVerticalIcon class="size-5" /></span>
			</div>
			<div class="flex items-center rounded-lg border border-border bg-background">
				<div class="min-w-0 flex-1 py-2 pl-3 text-[13.5px] text-muted-foreground">Follow up…</div>
				<div class="shrink-0 p-1">
					<span class="inline-flex h-7 items-center gap-1.5 rounded-md bg-border px-3 text-xs font-semibold text-inactive">
						Send<ArrowUpIcon class="size-3.5" />
					</span>
				</div>
			</div>
		</div>
	</div>
</div>
