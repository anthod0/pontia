<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';
	import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import GitMergeIcon from 'phosphor-svelte/lib/GitMergeIcon';
	import type { Snippet } from 'svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import * as Collapsible from '$lib/components/ui/collapsible/index.js';
	import { cn } from '$lib/utils';
	import type { DemoChange, DemoChangeGroup, DemoCommit, DemoGit } from './mockData';

	/**
	 * One Workspace's Git panel, laid out like the Dashboard's: Changes and History in a segmented control,
	 * then a file's diff, a commit, or a commit file's diff one level down. Esc steps back one level.
	 */
	let { git, heading, hint, file = $bindable(null) }: {
		git: DemoGit;
		/** The Workspace name or picker at the start of the header. */
		heading: Snippet;
		/** The keyboard hint under the change list. */
		hint: string;
		file?: DemoChange | null;
	} = $props();
	let element = $state<HTMLElement>();

	const groups: { group: DemoChangeGroup; label: string }[] = [
		{ group: 'staged', label: 'Staged' },
		{ group: 'unstaged', label: 'Unstaged' },
		{ group: 'untracked', label: 'Untracked' }
	];
	const statusTone: Record<DemoChange['status'], string> = { M: 'text-(color:--demo-git-modified)', A: 'text-(color:--demo-git-added)', D: 'text-destructive', '?': 'text-(color:--demo-git-untracked)' };
	const lineTone = { '+': 'bg-(color:--demo-git-added)/12', '-': 'bg-destructive/12', ' ': '' } as const;
	const markTone = { '+': 'text-(color:--demo-git-added)', '-': 'text-destructive', ' ': '' } as const;
	const iconButton = 'flex size-7 shrink-0 items-center justify-center rounded-full text-foreground hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary @max-3xl/demo:size-11';
	const row = 'flex h-[30px] w-full min-w-0 items-center gap-2 rounded-lg pr-2 pl-[22px] text-left text-[13px] outline-hidden hover:bg-muted focus-visible:bg-muted focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset @max-3xl/demo:h-11';
	const segment = 'h-full rounded-full px-3 text-xs data-active:bg-background data-active:shadow-sm dark:data-active:border-transparent dark:data-active:bg-[#45403d]';

	let view = $state('changes');
	let collapsed = $state<Partial<Record<DemoChangeGroup, boolean>>>({});
	let commit = $state<DemoCommit | null>(null);
	let commitFile = $state<DemoCommit['files'][number] | null>(null);

	const visibleGroups = $derived(groups.map((group) => ({ ...group, files: git.changes.filter((change) => change.group === group.group) })).filter((group) => group.files.length));
	const days = $derived(git.commits.reduce<{ label: string; commits: DemoCommit[] }[]>((list, item) => {
		if (list.at(-1)?.label !== item.day) list.push({ label: item.day, commits: [] });
		list.at(-1)!.commits.push(item);
		return list;
	}, []));

	function count(diff: string, mark: '+' | '-'): number {
		return diff.split('\n').filter((line) => line.startsWith(mark)).length;
	}

	/** Steps back one level, as the phone layout's back button does. Returns false at the top level. */
	export function goBack(): boolean {
		if (!file && !commit) return false;
		back();
		return true;
	}

	/** The phone layout's stack title for the current level. */
	export function title(): { title: string; subtitle?: string } | null {
		if (file) return { title: split(file.path).name, subtitle: [file.status, groups.find((item) => item.group === file!.group)!.label, split(file.path).dir].filter(Boolean).join(' · ') };
		if (commit && commitFile) return { title: split(commitFile.path).name, subtitle: [commitFile.status, commit.hash, split(commitFile.path).dir].filter(Boolean).join(' · ') };
		if (commit) return { title: `Commit ${commit.hash}` };
		return null;
	}

	function focusRow(key: string | undefined) {
		queueMicrotask(() => element?.querySelector<HTMLElement>(`[data-git-row="${key}"]`)?.focus({ preventScroll: true }));
	}

	// Each step back returns focus to the row it was opened from.
	function back() {
		if (commitFile) {
			const previous = commitFile.path;
			commitFile = null;
			focusRow(previous);
		} else if (commit) {
			const previous = commit.hash;
			commit = null;
			focusRow(previous);
		} else {
			const previous = file?.path;
			file = null;
			focusRow(previous);
		}
	}

	function enter(next: () => void) {
		next();
		element?.focus({ preventScroll: true });
	}

	/** Up and Down move between rows. */
	function moveFocus(event: KeyboardEvent) {
		if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
		const rows = [...(event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[data-git-row]')];
		event.preventDefault();
		const index = rows.indexOf(document.activeElement as HTMLElement);
		rows[index < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, index + (event.key === 'ArrowDown' ? 1 : -1)))]?.focus({ preventScroll: true });
	}

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

{#snippet counts(diff: string, size: string)}
	<span class={cn('flex shrink-0 gap-1.5 font-mono tabular-nums', size)}>
		{#if count(diff, '+')}<span class="text-(color:--demo-git-added)">+{count(diff, '+')}</span>{/if}
		{#if count(diff, '-')}<span class="text-destructive">−{count(diff, '-')}</span>{/if}
	</span>
{/snippet}

{#snippet fileRow(item: Omit<DemoChange, 'group'>, label: string, onopen: () => void)}
	{@const parts = split(item.path)}
	<li>
		<button type="button" data-git-row={item.path} class={row} title={item.path} aria-label={label} onclick={onopen}>
			<span class={cn('w-3.5 shrink-0 text-center font-mono text-[11px] font-medium', statusTone[item.status])} aria-hidden="true">{item.status}</span>
			<span class={cn('max-w-[60%] shrink-0 truncate', item.status === 'D' && 'text-muted-foreground')}>{parts.name}</span>
			<span class="min-w-0 flex-1 truncate text-left text-xs text-muted-foreground" dir="rtl"><bdi>{parts.dir}</bdi></span>
			{@render counts(item.diff, 'text-[11px]')}
		</button>
	</li>
{/snippet}

{#snippet diffHeader(path: string, backLabel: string, context: Snippet, diff: string)}
	{@const parts = split(path)}
	<header class="flex h-[52px] shrink-0 items-center gap-2 border-b border-border px-2 @max-3xl/demo:hidden">
		<button type="button" class={iconButton} aria-label={backLabel} title={backLabel} onclick={back}><ArrowLeftIcon class="size-4" /></button>
		<div class="min-w-0 flex-1" title={path}>
			<h2 class="git-heading truncate font-medium text-heading">{parts.name}</h2>
			<div class="flex min-w-0 items-center gap-1 text-[11.5px] text-muted-foreground">
				{@render context()}
				{#if parts.dir}<span aria-hidden="true">·</span><span class="truncate font-mono">{parts.dir}</span>{/if}
			</div>
		</div>
		{@render counts(diff, 'text-xs')}
		<span class={iconButton} aria-hidden="true"><ArrowClockwiseIcon class="size-4" /></span>
	</header>
{/snippet}

{#snippet diffBody(diff: string)}
	<div class="demo-scroll min-h-0 flex-1 overflow-auto">
		<div class="min-w-max font-mono text-xs leading-5">
			{#each diffLines(diff) as line, index (index)}
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
{/snippet}

<div bind:this={element} tabindex="-1" class="flex min-h-0 flex-1 flex-col outline-none">
	{#if file}
		{@const current = file}
		{#snippet context()}<span class={statusTone[current.status]}>{current.status}</span><span aria-hidden="true">·</span><span>{groups.find((item) => item.group === current.group)!.label}</span>{/snippet}
		{@render diffHeader(current.path, 'Back to changes (Esc)', context, current.diff)}
		{@render diffBody(current.diff)}
	{:else if commit && commitFile}
		{@const current = commitFile}
		{@const hash = commit.hash}
		{#snippet context()}<span class={statusTone[current.status]}>{current.status}</span><span aria-hidden="true">·</span><span>{hash}</span>{/snippet}
		{@render diffHeader(current.path, 'Back to commit (Esc)', context, current.diff)}
		{@render diffBody(current.diff)}
	{:else if commit}
		{@const current = commit}
		<header class="flex h-12 shrink-0 items-center gap-1 border-b border-border px-2 @max-3xl/demo:hidden">
			<button type="button" class={iconButton} aria-label="Back to history (Esc)" title="Back to history (Esc)" onclick={back}><ArrowLeftIcon class="size-4" /></button>
			<h2 class="git-heading min-w-0 flex-1 truncate px-1 font-medium text-heading">Commit {current.hash}</h2>
			<span class={iconButton} aria-hidden="true"><CopyIcon class="size-4" /></span>
		</header>
		<div class="demo-scroll min-h-0 flex-1 overflow-auto">
			<section class="space-y-2 border-b border-border px-4 py-3" aria-label="Commit message">
				<p class="text-sm font-medium break-words text-heading">{current.subject}</p>
				{#if current.body}<p class="text-xs leading-5 break-words text-muted-foreground">{current.body}</p>{/if}
				<dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs">
					<dt class="text-muted-foreground">Author</dt><dd class="min-w-0 truncate">{current.author}</dd>
					<dt class="text-muted-foreground">Date</dt><dd>{current.day === 'today' ? 'Today' : current.day === 'yesterday' ? 'Yesterday' : current.day}, {current.time}</dd>
					<dt class="text-muted-foreground">SHA</dt><dd class="min-w-0 truncate font-mono">{current.hash}</dd>
					<dt class="text-muted-foreground">Parents</dt><dd class="font-mono">{current.parents.join(' ')}</dd>
				</dl>
			</section>
			<div class="flex items-center gap-2 px-4 pt-3 pb-1 text-xs font-medium text-muted-foreground">
				<span class="text-foreground">Changed files</span><span class="tabular-nums">{current.files.length}</span>
				{#if current.parents.length > 1}<span class="ml-auto font-normal">Compared with the first parent</span>{/if}
			</div>
			<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
			<ul class="px-2 pb-2" onkeydown={moveFocus}>
				{#each current.files as item (item.path)}
					{@render fileRow(item, item.path, () => enter(() => (commitFile = item)))}
				{/each}
			</ul>
		</div>
	{:else}
		<Tabs.Root bind:value={view} class="min-h-0 flex-1 flex-col gap-0">
			<header class="grid shrink-0 grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-x-2 border-b border-border py-2 pr-2 pl-4 @max-3xl/demo:grid-cols-[minmax(0,1fr)_auto] @max-3xl/demo:gap-y-1 @max-3xl/demo:px-2">
				<div class="flex min-w-0 items-center gap-2 @max-3xl/demo:hidden">{@render heading()}</div>
				<Tabs.List aria-label="Git views" class="h-[30px] shrink-0 gap-0 rounded-full p-0.5 group-data-horizontal/tabs:h-[30px] @max-3xl/demo:order-last @max-3xl/demo:col-span-2 @max-3xl/demo:h-10 @max-3xl/demo:w-full @max-3xl/demo:group-data-horizontal/tabs:h-10">
					<Tabs.Trigger value="changes" class={segment}>Changes<span class="font-normal text-muted-foreground tabular-nums">{git.changes.length}</span></Tabs.Trigger>
					<Tabs.Trigger value="history" class={segment}>History</Tabs.Trigger>
				</Tabs.List>
				<span class={cn(iconButton, 'size-8 @max-3xl/demo:order-2')} aria-hidden="true"><ArrowClockwiseIcon class="size-4" /></span>
				<div class="col-span-3 flex h-6 min-w-0 items-center gap-1.5 text-xs text-muted-foreground @max-3xl/demo:order-1 @max-3xl/demo:col-span-1 @max-3xl/demo:h-11">
					{#if view === 'history'}
						<span class="flex h-6 min-w-0 items-center gap-1.5 rounded-full bg-muted px-2" title="Branch to show: {git.branch}">
							<GitBranchIcon class="size-4 shrink-0" />
							<span class="min-w-0 truncate font-mono text-[13px] text-foreground">{git.branch}</span>
							<span class="shrink-0">Current</span>
							<CaretDownIcon class="size-3.5 shrink-0" />
						</span>
					{:else}
						<GitBranchIcon class="size-3.5 shrink-0" />
						<span class="truncate font-mono font-medium text-foreground">{git.branch}</span>
						{#if git.ahead}<span class="shrink-0 font-mono text-primary" title={`${git.ahead} ahead`}>↑{git.ahead}</span>{/if}
						{#if git.behind}<span class="shrink-0 font-mono text-(color:--demo-git-behind)" title={`${git.behind} behind`}>↓{git.behind}</span>{/if}
						<span aria-hidden="true">·</span><span class="min-w-0 truncate font-mono">{git.upstream}</span>
					{/if}
					<span class="ml-auto shrink-0 tabular-nums @max-3xl/demo:hidden">Updated just now</span>
				</div>
			</header>
			<Tabs.Content value="changes" class="flex min-h-0 flex-col">
				<div class="demo-scroll min-h-0 flex-1 overflow-auto">
					{#if visibleGroups.length}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="px-2 py-1" onkeydown={moveFocus}>
							{#each visibleGroups as group (group.group)}
								<Collapsible.Root open={!collapsed[group.group]} onOpenChange={(open) => (collapsed[group.group] = !open)} class="py-1">
									<Collapsible.Trigger class="group sticky top-0 z-10 flex h-[30px] w-full items-center gap-1.5 rounded-md bg-surface px-2 text-xs font-medium text-muted-foreground outline-hidden hover:bg-muted focus-visible:bg-muted focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset @max-3xl/demo:h-9">
										<CaretDownIcon class="size-3 transition-transform group-data-[state=closed]:-rotate-90" />
										<span>{group.label}</span>
										<span class="font-normal tabular-nums">{group.files.length}</span>
									</Collapsible.Trigger>
									<Collapsible.Content>
										<ul>
											{#each group.files as change (change.path)}
												{@render fileRow(change, `${group.label}: ${change.path}`, () => enter(() => (file = change)))}
											{/each}
										</ul>
									</Collapsible.Content>
								</Collapsible.Root>
							{/each}
						</div>
						<p class="px-4 py-2 text-xs text-muted-foreground @max-3xl/demo:hidden">{hint}</p>
					{:else}
						<div class="flex flex-col items-center gap-2 p-8 text-center">
							<span class="flex size-12 items-center justify-center rounded-xl bg-muted text-muted-foreground"><CheckIcon class="size-6" /></span>
							<p class="text-sm font-medium text-heading">No uncommitted changes</p>
							<p class="text-sm text-muted-foreground">The working tree matches HEAD.</p>
						</div>
					{/if}
				</div>
			</Tabs.Content>
			<Tabs.Content value="history" class="flex min-h-0 flex-col">
				<div class="demo-scroll min-h-0 flex-1 overflow-auto">
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="relative px-2 pb-2 before:absolute before:top-[52px] before:bottom-[30px] before:left-[21px] before:w-px before:bg-border" onkeydown={moveFocus}>
						{#each days as day (day.label)}
							<h3 class="flex h-[30px] items-center pl-8 text-xs font-medium text-muted-foreground">{day.label}</h3>
							<ul aria-label={day.label}>
								{#each day.commits as item (item.hash)}
									{@const unpushed = !item.pushed}
									{@const merge = item.parents.length > 1}
									<li>
										<button type="button" data-git-row={item.hash} onclick={() => enter(() => (commit = item))}
											class="group relative flex h-11 w-full min-w-0 items-center gap-2 rounded-lg pr-2 pl-8 text-left outline-hidden hover:bg-muted focus-visible:bg-muted focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset">
											<span aria-hidden="true" class={cn('absolute left-[9px] size-[9px] rounded-full border border-current', unpushed ? 'text-amber-700 dark:text-amber-400' : 'text-muted-foreground', merge ? 'bg-background' : 'bg-current')}></span>
											<span class="min-w-0 flex-1">
												<span class="flex items-center gap-1.5 text-[13px]">{#if merge}<GitMergeIcon aria-label="merge" class="size-3.5 shrink-0 text-muted-foreground" />{/if}<span class="truncate">{item.subject}</span></span>
												<span class="flex items-center gap-1 text-xs text-muted-foreground"><span class="truncate">{item.author}</span> · <span class="shrink-0 tabular-nums">{item.time}</span></span>
											</span>
											<span title={unpushed ? 'Unpushed' : 'Pushed'} class={cn('shrink-0 font-mono text-[11.5px]', unpushed ? 'font-medium text-amber-700 dark:text-amber-400' : 'text-muted-foreground')}>{#if unpushed}<span aria-hidden="true">↑ </span>{/if}{item.hash}</span>
										</button>
									</li>
								{/each}
							</ul>
						{/each}
					</div>
				</div>
			</Tabs.Content>
		</Tabs.Root>
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
