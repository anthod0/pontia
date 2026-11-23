<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';
	import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import ClockIcon from 'phosphor-svelte/lib/ClockIcon';
	import CloudArrowUpIcon from 'phosphor-svelte/lib/CloudArrowUpIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import type { Snippet } from 'svelte';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import * as Collapsible from '$lib/components/ui/collapsible/index.js';
	import { cn } from '$lib/utils';
	import type { DemoChange, DemoChangeGroup, DemoGit } from './mockData';

	/** One Workspace's changes: the change list under `header`, or the diff of the opened file. */
	let { git, header, hint, file = $bindable(null) }: {
		git: DemoGit;
		header: Snippet;
		/** The keyboard hint in the footer. */
		hint: string;
		file?: DemoChange | null;
	} = $props();
	let element = $state<HTMLElement>();
	let list = $state<HTMLElement>();

	// Group order, symbols and colours follow the Dashboard's Git panel.
	const groups: { group: DemoChangeGroup; label: string; symbol: string; tone: string }[] = [
		{ group: 'staged', label: 'Staged', symbol: '+', tone: 'text-(color:--demo-git-added)' },
		{ group: 'unstaged', label: 'Unstaged', symbol: '~', tone: 'text-(color:--demo-git-modified)' },
		{ group: 'untracked', label: 'Untracked', symbol: '?', tone: 'text-(color:--demo-git-untracked)' }
	];
	const statusTone: Record<DemoChange['status'], string> = { M: 'text-(color:--demo-git-modified)', A: 'text-(color:--demo-git-added)', '?': 'text-(color:--demo-git-untracked)' };
	const lineTone = { '+': 'bg-(color:--demo-git-added)/12', '-': 'bg-destructive/12', ' ': '' } as const;
	const markTone = { '+': 'text-(color:--demo-git-added)', '-': 'text-destructive', ' ': '' } as const;
	const iconButton = 'flex size-7 items-center justify-center rounded-md text-foreground hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary';

	let collapsed = $state<Partial<Record<DemoChangeGroup, boolean>>>({});

	function count(change: DemoChange, mark: '+' | '-'): number {
		return change.diff.split('\n').filter((line) => line.startsWith(mark)).length;
	}

	/** Esc returns from a diff to the list. The parent forwards keys from the element that holds focus. */
	export function keydown(event: KeyboardEvent) {
		if (event.key !== 'Escape' || event.defaultPrevented || !file) return;
		back();
		event.preventDefault();
	}

	// The clicked row unmounts, so keep focus in the panel for Esc.
	function open(change: DemoChange) {
		file = change;
		element?.closest<HTMLElement>('[tabindex]')?.focus();
	}

	function back() {
		const previous = file;
		file = null;
		queueMicrotask(() => element?.querySelector<HTMLElement>(`[data-path="${previous?.path}"]`)?.focus());
	}

	/** Up and Down move between file rows. */
	function listKeydown(event: KeyboardEvent) {
		if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
		const rows = [...list!.querySelectorAll<HTMLElement>('[data-path]')];
		event.preventDefault();
		const index = rows.indexOf(document.activeElement as HTMLElement);
		rows[index < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, index + (event.key === 'ArrowDown' ? 1 : -1)))]?.focus();
	}
	const visibleGroups = $derived(groups.map((group) => ({ ...group, files: git.changes.filter((change) => change.group === group.group) })).filter((group) => group.files.length));

	function split(path: string): { name: string; dir: string } {
		const slash = path.lastIndexOf('/');
		return slash < 0 ? { name: path, dir: '' } : { name: path.slice(slash + 1), dir: path.slice(0, slash) };
	}

	type DiffLine = { kind: '@' | '+' | '-' | ' '; text: string; old: number | null; new: number | null };

	function diffLines(diff: string): DiffLine[] {
		let oldLine = 0;
		let newLine = 0;
		return diff.split('\n').map((line) => {
			if (line.startsWith('@@')) {
				const match = line.match(/-(\d+)(?:,\d+)? \+(\d+)/);
				oldLine = Number(match?.[1] ?? 0);
				newLine = Number(match?.[2] ?? 0);
				return { kind: '@', text: line, old: null, new: null };
			}
			const kind = line[0] === '+' || line[0] === '-' ? line[0] : ' ';
			const entry: DiffLine = { kind, text: line.slice(1), old: kind === '+' ? null : oldLine, new: kind === '-' ? null : newLine };
			if (kind !== '+') oldLine++;
			if (kind !== '-') newLine++;
			return entry;
		});
	}
</script>

<div bind:this={element} class="flex min-h-0 flex-1 flex-col">
	{#if file}
		{@const parts = split(file.path)}
		{@const group = groups.find((item) => item.group === file!.group)!}
		<header class="flex h-12 shrink-0 items-center gap-1 border-b border-border px-2">
			<button type="button" class={iconButton} aria-label="Back to changes (Esc)" title="Back to changes (Esc)" onclick={back}><ArrowLeftIcon class="size-4" /></button>
			<div class="flex min-w-0 flex-1 items-baseline gap-2 px-1" title={file.path}>
				<h2 class="git-heading min-w-0 shrink truncate text-sm font-medium text-heading">{parts.name}</h2>
				{#if parts.dir}<span class="min-w-0 flex-1 truncate text-left font-mono text-xs text-muted-foreground" dir="rtl"><bdi>{parts.dir}</bdi></span>{/if}
			</div>
			<span class="flex size-7 items-center justify-center" aria-hidden="true"><ArrowClockwiseIcon class="size-4" /></span>
		</header>
		<div class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-1.5 text-xs">
			<Badge variant="outline" class={cn('font-normal', group.tone)}>{group.label}</Badge>
			<span class="ml-auto flex shrink-0 gap-1.5 font-mono tabular-nums">
				{#if count(file, '+')}<span class="text-(color:--demo-git-added)">+{count(file, '+')}</span>{/if}
				{#if count(file, '-')}<span class="text-destructive">−{count(file, '-')}</span>{/if}
			</span>
		</div>
		<div class="demo-scroll min-h-0 flex-1 overflow-auto">
			<div class="min-w-max font-mono text-xs leading-5">
				{#each diffLines(file.diff) as line, index (index)}
					{#if line.kind === '@'}
						<div class="flex bg-muted/60 text-muted-foreground">
							<span class="sticky left-0 w-[4.5rem] shrink-0 bg-muted"></span>
							<span class="px-3 py-0.5 whitespace-pre">{line.text}</span>
						</div>
					{:else}
						<div class={cn('flex', lineTone[line.kind])}>
							<span class="sticky left-0 flex w-[4.5rem] shrink-0 bg-surface text-right text-muted-foreground/70 tabular-nums select-none">
								<span class={cn('w-9 pr-1.5', lineTone[line.kind])}>{line.old ?? ''}</span>
								<span class={cn('w-9 pr-1.5', lineTone[line.kind])}>{line.new ?? ''}</span>
							</span>
							<span class={cn('w-5 shrink-0 text-center select-none', markTone[line.kind])} aria-hidden="true">{line.kind === '-' ? '−' : line.kind}</span>
							<span class="pr-4 whitespace-pre text-foreground">{line.text}</span>
						</div>
					{/if}
				{/each}
			</div>
		</div>
	{:else}
		{@render header()}
		<div class="demo-scroll min-h-0 flex-1 overflow-auto">
			<div class="border-b border-border px-4 py-3">
				<div class="flex min-w-0 items-center gap-2 text-sm">
					<GitBranchIcon class="size-4 shrink-0 text-muted-foreground" />
					<span class="min-w-0 truncate font-mono text-[13px] font-medium text-heading">{git.branch}</span>
					<span class="flex shrink-0 gap-1.5 font-mono text-xs">
						{#if git.ahead}<span class="text-primary" title={`${git.ahead} ahead`}>↑{git.ahead}</span>{/if}
						{#if git.behind}<span class="text-(color:--demo-git-behind)" title={`${git.behind} behind`}>↓{git.behind}</span>{/if}
						{#if !git.ahead && !git.behind}<span class="text-muted-foreground">Up to date</span>{/if}
					</span>
				</div>
				<div class="mt-1 flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
					<CloudArrowUpIcon class="size-3.5 shrink-0" />
					<span class="min-w-0 truncate font-mono">{git.upstream}</span>
				</div>
			</div>
			{#if visibleGroups.length}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div bind:this={list} class="px-2 py-1" onkeydown={listKeydown}>
					{#each visibleGroups as group (group.group)}
						<Collapsible.Root open={!collapsed[group.group]} onOpenChange={(open) => (collapsed[group.group] = !open)} class="py-1">
							<Collapsible.Trigger class="group flex h-[30px] w-full items-center gap-1.5 rounded-md px-2 text-xs font-medium text-muted-foreground outline-hidden hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary">
								<CaretDownIcon class="size-3 transition-transform group-data-[state=closed]:-rotate-90" />
								<span class={cn('w-3.5 text-center font-mono', group.tone)} aria-hidden="true">{group.symbol}</span>
								<span class="text-foreground">{group.label}</span>
								<span class="tabular-nums">{group.files.length}</span>
							</Collapsible.Trigger>
							<Collapsible.Content>
								<ul>
									{#each group.files as change (change.path)}
										{@const parts = split(change.path)}
										<li>
											<button
												type="button"
												data-path={change.path}
												class="flex h-[30px] w-full min-w-0 items-center gap-2 rounded-md pr-2 pl-2.5 text-left text-[13px] outline-hidden hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary"
												title={change.path}
												aria-label={`${group.label}: ${change.path}`}
												onclick={() => open(change)}
											>
												<span class={cn('w-3.5 shrink-0 text-center font-mono text-[11px] font-medium', statusTone[change.status])} aria-hidden="true">{change.status}</span>
												<span class="max-w-[60%] shrink-0 truncate">{parts.name}</span>
												<span class="min-w-0 flex-1 truncate text-left text-xs text-muted-foreground" dir="rtl"><bdi>{parts.dir}</bdi></span>
												<span class="flex shrink-0 gap-1.5 font-mono text-[11px] tabular-nums">
													{#if count(change, '+')}<span class="text-(color:--demo-git-added)">+{count(change, '+')}</span>{/if}
													{#if count(change, '-')}<span class="text-destructive">−{count(change, '-')}</span>{/if}
												</span>
											</button>
										</li>
									{/each}
								</ul>
							</Collapsible.Content>
						</Collapsible.Root>
					{/each}
				</div>
			{:else}
				<div class="flex flex-col items-center gap-2 p-8 text-center">
					<span class="flex size-12 items-center justify-center rounded-xl bg-muted text-muted-foreground"><CheckIcon class="size-6" /></span>
					<p class="text-sm font-medium text-heading">No uncommitted changes</p>
					<p class="text-sm text-muted-foreground">The working tree matches HEAD.</p>
				</div>
			{/if}
		</div>
		<footer class="flex h-9 shrink-0 items-center gap-1.5 border-t border-border px-4 text-xs text-muted-foreground">
			<ClockIcon class="size-3.5" />
			<span class="shrink-0">Updated just now</span>
			<span class="flex-1"></span>
			<span class="hidden truncate @7xl:inline">{hint}</span>
		</footer>
	{/if}
</div>

<style>
	/* The site's section headings style every h2; the panel keeps the Dashboard's size. */
	.git-heading {
		font-size: 14px;
		line-height: 20px;
		letter-spacing: normal;
	}
</style>
