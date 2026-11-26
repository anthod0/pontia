<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';
	import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';
	import CaretUpIcon from 'phosphor-svelte/lib/CaretUpIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import type { Snippet } from 'svelte';
	import type { DemoChange, DemoCommit, DemoGit } from './mockData';

	let { git, heading, file = $bindable(null) }: { git: DemoGit; heading: Snippet; hint: string; file?: DemoChange | null } = $props();
	let commit = $state<DemoCommit | null>(null);
	let commitFile = $state<DemoCommit['files'][number] | null>(null);
	let changesOpen = $state(true);
	let historyOpen = $state(true);

	const unstaged = $derived(git.changes.filter((item) => item.group === 'unstaged' || item.group === 'untracked'));
	const staged = $derived(git.changes.filter((item) => item.group === 'staged'));
	const groups = $derived([
		{ label: 'Staged', files: staged },
		{ label: 'Unstaged', files: unstaged }
	].filter((group) => group.files.length));
	const days = $derived(git.commits.reduce<{ label: string; commits: DemoCommit[] }[]>((list, item) => {
		if (list.at(-1)?.label !== item.day) list.push({ label: item.day, commits: [] });
		list.at(-1)!.commits.push(item);
		return list;
	}, []));
	const currentDiff = $derived(commitFile?.diff ?? file?.diff);
	const statusTone: Record<string, string> = { M: 'text-(--git-modified)', A: 'text-(--git-added)', D: 'text-(--git-deleted)', '?': 'text-(--git-added)' };

	export function goBack(): boolean {
		if (commitFile) { commitFile = null; return true; }
		if (file) { file = null; return true; }
		return false;
	}

	export function title(): { title: string; subtitle?: string } | null {
		const current = commitFile ?? file;
		if (!current) return null;
		return { title: split(current.path).name, subtitle: [current.status, commitFile ? commit?.hash : 'Unstaged', split(current.path).dir].filter(Boolean).join(' · ') };
	}

	function split(path: string): { name: string; dir: string } {
		const slash = path.lastIndexOf('/');
		return slash < 0 ? { name: path, dir: '' } : { name: path.slice(slash + 1), dir: path.slice(0, slash) };
	}

	function count(diff: string, mark: '+' | '-'): number { return diff.split('\n').filter((line) => line.startsWith(mark)).length; }

	type DiffLine = { kind: '@' | '+' | '-' | ' '; text: string; old: number | null; new: number | null };
	function diffLines(diff: string): DiffLine[] {
		let oldLine = 0;
		let newLine = 0;
		return diff.split('\n').map((line) => {
			if (line.startsWith('@@')) {
				const match = line.match(/-(\d+)(?:,\d+)? \+(\d+)/);
				oldLine = Number(match?.[1] ?? 0); newLine = Number(match?.[2] ?? 0);
				return { kind: '@', text: line, old: null, new: null };
			}
			const kind = line[0] === '+' || line[0] === '-' ? line[0] : ' ';
			const entry: DiffLine = { kind, text: line.slice(1), old: kind === '+' ? null : oldLine, new: kind === '-' ? null : newLine };
			if (kind !== '+') oldLine++; if (kind !== '-') newLine++;
			return entry;
		});
	}
</script>

{#snippet fileRow(item: Omit<DemoChange, 'group'>, onopen: () => void, selected = false)}
	{@const parts = split(item.path)}
	<button type="button" class={['grid h-[30px] w-full grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 text-left text-[13px] hover:bg-muted @max-3xl/demo:h-11', selected && 'bg-selected']} onclick={onopen}>
		<span class={['text-center font-mono text-[11px]', statusTone[item.status]]}>{item.status}</span>
		<span class="flex min-w-0 gap-2"><span class="max-w-[68%] truncate">{parts.name}</span><span class="truncate text-xs text-muted-foreground" dir="rtl">{parts.dir}</span></span>
		<span class="flex gap-1.5 font-mono text-[11px]">{#if count(item.diff, '+')}<span class="text-(--git-added)">+{count(item.diff, '+')}</span>{/if}{#if count(item.diff, '-')}<span class="text-(--git-deleted)">−{count(item.diff, '-')}</span>{/if}</span>
	</button>
{/snippet}

{#snippet sectionTitle(kind: 'changes' | 'history', title: string, open: boolean, toggle: () => void)}
	<div class={['sticky z-20 grid h-7 grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-x-2 border-border bg-surface px-4 text-xs font-semibold @max-3xl/demo:h-10', kind === 'changes' ? 'top-0' : 'top-7 bottom-[-12px] border-t @max-3xl/demo:top-10']} role="button" tabindex="0" onclick={toggle} onkeydown={(event) => { if (event.key === 'Enter') toggle(); }}>
		<CaretDownIcon class={['size-3', !open && '-rotate-90']} /><span>{title}{#if kind === 'changes'} <span class="font-normal text-muted-foreground">{git.changes.length}</span>{/if}</span>
		{#if kind === 'history'}<span class="flex h-[22px] items-center gap-1 rounded-full px-2 font-mono text-[11.5px] font-medium text-muted-foreground hover:bg-muted"><GitBranchIcon class="size-3" />{git.branch}<CaretDownIcon class="size-2.5" /></span>{/if}
	</div>
{/snippet}

{#snippet lists()}
	<div class="demo-scroll min-h-0 flex-1 overflow-auto pb-3">
		{@render sectionTitle('changes', 'Changes', changesOpen, () => (changesOpen = !changesOpen))}
		{#if changesOpen}<div class="px-2 py-1">{#each groups as group}{#if groups.length > 1}<div class="grid h-[26px] grid-cols-[16px_1fr] items-center gap-x-2 px-2 text-[11.5px] text-muted-foreground"><span></span>{group.label} {group.files.length}</div>{/if}{#each group.files as item}<div>{@render fileRow(item, () => { file = item; commit = null; commitFile = null; }, file === item)}</div>{/each}{/each}</div>{/if}
		{@render sectionTitle('history', 'History', historyOpen, () => (historyOpen = !historyOpen))}
		{#if historyOpen}<div class="relative px-2 pb-2 before:absolute before:top-9 before:bottom-5 before:left-[21px] before:w-px before:bg-border">{#each days as day}<h3 class="grid h-7 grid-cols-[16px_1fr] items-center gap-x-2 px-2 text-xs font-medium text-muted-foreground"><span></span>{day.label}</h3>{#each day.commits as item}<button type="button" class={['grid h-11 w-full grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 text-left hover:bg-muted', commit === item && 'bg-selected']} onclick={() => { commit = commit === item ? null : item; file = null; commitFile = null; }}><span class={['relative z-10 mx-auto size-2 rounded-full', item.pushed ? 'bg-muted-foreground/60' : 'bg-primary']}></span><span class="min-w-0"><span class="block truncate text-[13px]">{item.subject}</span><span class="text-xs text-muted-foreground">{item.author} · {item.time}</span></span><span class="font-mono text-[11px] text-muted-foreground">{item.hash}</span></button>{#if commit === item}<div class="ml-8 rounded-lg bg-[color-mix(in_srgb,var(--muted)_55%,var(--surface))] p-3"><p class="line-clamp-3 text-xs leading-5 text-muted-foreground">{item.body}</p><div class="flex items-center gap-1.5 py-2 text-xs text-muted-foreground"><span class="rounded-full bg-surface px-2 font-mono">{item.hash} <CopyIcon class="inline size-3" /></span><span>· {item.day}, {item.time}</span></div><div class="pb-1 text-xs font-medium">{item.files.length} files</div>{#each item.files as changed}<div>{@render fileRow(changed, () => (commitFile = changed))}</div>{/each}</div>{/if}{/each}{/each}</div>{/if}
	</div>
{/snippet}

{#snippet detail()}
	{#if currentDiff}
		{@const current = commitFile ?? file!}
		<header class="flex h-[52px] items-center gap-2 border-b px-2"><button type="button" class="narrow-back grid size-7 place-items-center rounded-full hover:bg-muted" onclick={goBack}><ArrowLeftIcon class="size-4" /></button><div class="min-w-0 flex-1"><div class="flex gap-1.5 text-[13px] font-medium"><span class={statusTone[current.status]}>{current.status}</span><span class="truncate">{split(current.path).name}</span></div><div class="text-[11.5px] text-muted-foreground">{split(current.path).dir} · {commitFile ? `Commit ${commit?.hash}` : 'Unstaged'}</div></div><span class="font-mono text-xs text-(--git-added)">+{count(currentDiff, '+')}</span><span class="font-mono text-xs text-(--git-deleted)">−{count(currentDiff, '-')}</span><CaretUpIcon class="size-4" /><CaretDownIcon class="size-4" /></header>
		<div class="demo-scroll min-h-0 flex-1 overflow-auto font-mono text-xs leading-5">{#each diffLines(currentDiff) as line}{#if line.kind === '@'}<div class="flex bg-muted/70 text-muted-foreground"><span class="sticky left-0 w-[8ch] bg-muted/70"></span><span class="px-3 py-0.5">{line.text}</span></div>{:else}<div class={['flex', line.kind === '+' ? 'bg-[color-mix(in_srgb,var(--diff-add)_13%,transparent)]' : line.kind === '-' ? 'bg-[color-mix(in_srgb,var(--diff-del)_9%,transparent)]' : '']}><span class={['sticky left-0 flex w-[8ch] shrink-0 border-l-2 bg-surface text-right text-muted-foreground/70', line.kind === '+' ? 'border-(--git-added)' : line.kind === '-' ? 'border-(--git-deleted)' : 'border-transparent']}><span class="w-[4ch]">{line.old ?? ''}</span><span class="w-[4ch]">{line.new ?? ''}</span></span><span class={['w-5 text-center', line.kind === '+' ? 'text-(--git-added)' : line.kind === '-' ? 'text-(--git-deleted)' : '']}>{line.kind === '-' ? '−' : line.kind}</span><span class="pr-4 whitespace-pre">{line.text}</span></div>{/if}{/each}</div>
	{:else if commit}
		<header class="flex h-[52px] items-center gap-2 border-b px-3 text-[13px] font-medium">Commit <span class="ml-auto rounded-full bg-muted px-2 font-mono text-[11.5px]">{commit.hash} <CopyIcon class="inline size-3" /></span></header><div class="p-4"><p class="text-sm font-medium">{commit.subject}</p><p class="mt-2 text-xs leading-5 text-muted-foreground">{commit.body}</p><p class="mt-2 text-xs text-muted-foreground">{commit.author} · {commit.day}, {commit.time}</p></div>
	{/if}
{/snippet}

<div class="git-demo flex min-h-0 flex-1 flex-col">
	<header class="flex h-11 shrink-0 items-center gap-1 border-b px-2"><div class="flex min-w-0 items-center">{@render heading()}</div><span class="mx-1 h-4 w-px bg-border"></span><span class="flex h-7 items-center gap-1.5 rounded-full px-2"><GitBranchIcon class="size-3.5 text-muted-foreground" /><span class="font-mono text-xs font-medium">{git.branch}</span>{#if git.ahead}<span class="font-mono text-xs text-primary">↑{git.ahead}</span>{/if}{#if git.behind}<span class="font-mono text-xs text-(--demo-git-behind)">↓{git.behind}</span>{/if}</span><span class="truncate font-mono text-[11.5px] text-muted-foreground">→ {git.upstream}</span><span class="flex-1"></span><ArrowClockwiseIcon class="size-4" /></header>
	<div class="wide-layout min-h-0 flex-1"><div class="left-pane flex min-h-0 flex-col">{@render lists()}</div><div class="right-pane min-h-0 flex-col">{@render detail()}</div></div>
	<div class="narrow-detail min-h-0 flex-1 flex-col">{@render detail()}</div>
</div>

<style>
	.git-demo { container-type: inline-size; --git-modified: #b57614; --git-added: #427b58; --git-deleted: #cc241d; --diff-add: #689d6a; --diff-del: #cc241d; }
	.wide-layout { display: contents; }
	.right-pane, .narrow-detail { display: none; }
	@container (min-width: 720px) { .wide-layout { display: grid; grid-template-columns: 360px minmax(0, 1fr); } .left-pane { border-right: 1px solid var(--border); } .right-pane { display: flex; } .narrow-detail { display: none !important; } .narrow-back { visibility: hidden; } }
	@container (max-width: 719px) { .left-pane { display: flex; } .wide-layout:has(+ .narrow-detail:not(:empty)) .left-pane { display: none; } .narrow-detail { display: flex; } }
	:global(.dark) .git-demo { --git-modified: #fabd2f; --git-added: #8ec07c; --git-deleted: #fb4934; --diff-add: #8ec07c; --diff-del: #fb4934; }
</style>
