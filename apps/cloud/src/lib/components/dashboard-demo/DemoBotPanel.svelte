<script lang="ts" module>
	export type PanelTab = 'files' | 'preview' | 'context' | 'todo' | 'search' | 'git';
</script>

<script lang="ts">
	import { Button } from '$lib/components/ui/button/index.js';
	import ArrowBendUpLeftIcon from 'phosphor-svelte/lib/ArrowBendUpLeftIcon';
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import ArrowUUpLeftIcon from 'phosphor-svelte/lib/ArrowUUpLeftIcon';
	import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon';
	import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon';
	import EyeSlashIcon from 'phosphor-svelte/lib/EyeSlashIcon';
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import FolderIcon from 'phosphor-svelte/lib/FolderIcon';
	import FolderSimpleIcon from 'phosphor-svelte/lib/FolderSimpleIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import ListChecksIcon from 'phosphor-svelte/lib/ListChecksIcon';
	import MagnifyingGlassIcon from 'phosphor-svelte/lib/MagnifyingGlassIcon';
	import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon';
	import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon';
	import XIcon from 'phosphor-svelte/lib/XIcon';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import * as Collapsible from '$lib/components/ui/collapsible/index.js';
	import * as Empty from '$lib/components/ui/empty/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { cn } from '$lib/utils';
	import BotAvatarMark from '$lib/components/bot-story/BotAvatarMark.svelte';
	import DemoFilesTab from './DemoFilesTab.svelte';
	import DemoGitChanges from './DemoGitChanges.svelte';
	import DemoBotContextList from './DemoBotContextList.svelte';
	import DemoPreviewTab from './DemoPreviewTab.svelte';
	import DemoStackHeader from './DemoStackHeader.svelte';
	import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon';
	import type { DemoBot, DemoBotEntry, DemoTodo, DemoWorkspace } from './mockData';
	import { plainText } from './text';

	/** The Bot chat's side panel. The chat page keeps its state while the panel is closed. */
	let {
		bot,
		workspaces,
		count,
		element = $bindable(),
		tab = $bindable(),
		file = $bindable(),
		preview = $bindable(),
		phone,
		selected = $bindable(),
		recent = $bindable(),
		query = $bindable(),
		searchSelected = $bindable(),
		workspaceId = $bindable(),
		onopenfile,
		onjump,
		onquote,
		onreply,
		ontodochange,
		onclose
	}: {
		bot: DemoBot;
		workspaces: DemoWorkspace[];
		count: number;
		element?: HTMLElement;
		tab: PanelTab;
		file: string | null;
		/** The localhost address the Preview tab shows. */
		preview: string | null;
		phone: boolean;
		selected: string | null;
		/** Recently opened files, newest first. */
		recent: string[];
		query: string;
		searchSelected: string | null;
		/** The Workspace the Git tab shows. */
		workspaceId: string | null;
		onopenfile: (path: string) => void;
		onjump: (id: string) => void;
		onquote: (entry: DemoBotEntry) => void;
		onreply: (text: string, quote: string) => void;
		ontodochange: (entry: BotReply, todo: DemoTodo) => void;
		onclose: () => void;
	} = $props();

	type BotReply = Extract<DemoBotEntry, { kind: 'bot' }>;
	type Message = Exclude<DemoBotEntry, { kind: 'notice' }>;

	let git = $state<DemoGitChanges>();
	let field = $state<HTMLInputElement>();
	let reply = $state<{ id: string; text: string } | null>(null);
	let replyButton: HTMLButtonElement;

	$effect(() => { if (tab === 'search') field?.focus({ preventScroll: true }); });

	function closeReply() {
		reply = null;
		replyButton.focus({ preventScroll: true });
	}

	const replies = $derived(bot.entries.filter((entry): entry is BotReply => entry.kind === 'bot'));
	const openTodos = $derived(replies.filter((entry) => entry.todo === 'open'));
	const resolvedTodos = $derived(replies.filter((entry) => entry.todo && entry.todo !== 'open'));
	const messages = $derived(bot.entries.filter((entry): entry is Message => entry.kind !== 'notice'));
	// The selected message with the messages it quotes or that quote it, as the Dashboard links them.
	const context = $derived.by(() => {
		if (!selected) return [];
		const target = messages.find((entry) => entry.id === selected);
		const quotes = target?.kind === 'user' ? target.quote : undefined;
		return messages.filter((entry) => entry.id === selected || entry.id === quotes || (entry.kind === 'user' && entry.quote === selected));
	});
	const contextSubtitle = $derived(context.length ? `${context.length} messages · ${context[0].time} – ${context.at(-1)!.time}` : bot.name);
	const results = $derived(query.trim() ? messages.filter((entry) => plainText(entry).toLowerCase().includes(query.trim().toLowerCase())).reverse() : []);
	const workspace = $derived(workspaces.find((item) => item.id === workspaceId));

	const tabTitles: Record<PanelTab, string> = { files: 'Files', preview: 'Preview', context: 'Context', todo: 'Todo', search: 'Search', git: 'Git' };
	const tabTrigger = 'h-full flex-none px-1 text-xs @[32rem]/panel:px-2 @[32rem]/panel:text-sm text-foreground/60 hover:text-foreground data-[state=active]:text-foreground data-[state=active]:after:opacity-100 [&_svg]:size-4';
	const smallButton = 'inline-flex h-6 items-center gap-1 rounded-md px-2 text-xs font-medium hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary [&_svg]:size-3.5';
	const iconButton = 'flex size-7 shrink-0 items-center justify-center rounded-full text-foreground hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary';

	const stack = $derived(tab === 'git' ? git?.title() : null);

	function back() {
		if (tab === 'git' && git?.goBack()) return;
		if (tab === 'git' && phone && workspaceId) workspaceId = null;
		else if (tab === 'files' && file) file = null;
		else onclose();
	}

	function highlight(text: string): { text: string; match: boolean }[] {
		const needle = query.trim().toLowerCase();
		const parts: { text: string; match: boolean }[] = [];
		let at = 0;
		for (let index = text.toLowerCase().indexOf(needle); needle && index >= 0; index = text.toLowerCase().indexOf(needle, at)) {
			if (index > at) parts.push({ text: text.slice(at, index), match: false });
			parts.push({ text: text.slice(index, index + needle.length), match: true });
			at = index + needle.length;
		}
		if (at < text.length) parts.push({ text: text.slice(at), match: false });
		return parts;
	}
</script>

{#snippet avatar(entry: DemoBotEntry, size: string)}
	{#if entry.kind === 'user'}
		<span aria-hidden="true" class={cn('grid shrink-0 place-items-center rounded-full bg-border text-[10px] font-semibold text-heading', size)}>Y</span>
	{:else}
		<span class={cn('shrink-0', size)}><BotAvatarMark shape={bot.shape} color={bot.color} /></span>
	{/if}
{/snippet}

{#snippet empty(Icon: typeof FileTextIcon, title: string, description?: string)}
	<Empty.Root class="min-h-0 flex-none p-8">
		<Empty.Header>
			<Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><Icon class="size-6" /></Empty.Media>
			<Empty.Title class="text-sm">{title}</Empty.Title>
			{#if description}<Empty.Description class="text-sm">{description}</Empty.Description>{/if}
		</Empty.Header>
	</Empty.Root>
{/snippet}

{#snippet todoCard(entry: BotReply)}
	{@const done = entry.todo !== 'open' ? entry.todo : null}
	<li class={cn('rounded-xl border p-3', done ? 'border-transparent bg-muted/50' : 'border-border bg-background')}>
		<button type="button" class="block w-full rounded-md text-left focus-visible:ring-2 focus-visible:ring-primary" onclick={() => onjump(entry.id)}>
			<span class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
				<span class="size-4"><BotAvatarMark shape={bot.shape} color={bot.color} /></span>
				<span class="tabular-nums">{entry.time}</span>
				{#if done}<span class="ml-auto">{done.by === 'user' ? 'Resolved by you' : `Closed by ${bot.name}`} · {done.at}</span>{/if}
			</span>
			<span class={cn('text-[13px] leading-5 [overflow-wrap:anywhere]', done ? 'line-clamp-2 text-muted-foreground' : 'line-clamp-3')}>{plainText(entry)}</span>
		</button>
		<div class={cn('mt-2 flex flex-wrap items-center gap-1', done && 'text-muted-foreground')}>
			<button type="button" class={smallButton} onclick={() => { selected = entry.id; tab = 'context'; }}><TreeStructureIcon />Context</button>
			{#if !done}
				<button type="button" class={cn(smallButton, 'ml-auto')} onclick={() => {
					if (reply?.id === entry.id) reply = null;
					ontodochange(entry, { by: 'user', at: 'Just now' });
				}}><EyeSlashIcon />Ignore</button>
				<button type="button" class={smallButton} onclick={() => onquote(entry)}><QuotesIcon />Quote</button>
				<Button variant="outline" size="xs" aria-expanded={reply?.id === entry.id} aria-controls={`demo-reply-${entry.id}`} onclick={(event) => {
					replyButton = event.currentTarget as HTMLButtonElement;
					reply = reply?.id === entry.id ? null : { id: entry.id, text: '' };
				}}><ArrowBendUpLeftIcon />Reply</Button>
			{:else if done.by === 'user'}
				<button type="button" class={cn(smallButton, 'ml-auto')} onclick={() => ontodochange(entry, 'open')}><ArrowUUpLeftIcon />Reopen</button>
			{/if}
		</div>
		{#if !done && reply?.id === entry.id}
			{@const draft = reply}
			<form id={`demo-reply-${entry.id}`} aria-label="Reply" class="mt-2 rounded-lg border border-border bg-background p-1" onsubmit={(event) => {
				event.preventDefault();
				if (!draft.text.trim()) return;
				onreply(draft.text.trim(), entry.id);
				closeReply();
			}}>
				<textarea aria-label="Message" placeholder="Message" rows="2" bind:value={draft.text} {@attach node => node.focus({ preventScroll: true })} class="w-full resize-none px-2 py-1 text-sm outline-none" onkeydown={(event) => {
					if (event.isComposing) return;
					if (event.key === 'Escape') {
						event.preventDefault();
						closeReply();
					}
					if (event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.metaKey) {
						event.preventDefault();
						event.currentTarget.form?.requestSubmit();
					}
				}}></textarea>
				<div class="flex justify-end gap-1">
					<Button variant="ghost" size="xs" onclick={closeReply}>Cancel</Button>
					<Button type="submit" size="xs" disabled={!draft.text.trim()}>Send</Button>
				</div>
			</form>
		{/if}
	</li>
{/snippet}

{#snippet workspaceLabel(item: DemoWorkspace)}
	<span class="flex min-w-0 flex-1 flex-col text-left">
		<span class="truncate">{item.name}</span>
		<span class="truncate font-mono text-[11px] text-muted-foreground">{item.path}</span>
	</span>
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside
	tabindex="-1"
	{@attach node => node.focus({ preventScroll: true })}
	onkeydown={(event) => {
		if (event.key !== 'Escape' || event.defaultPrevented) return;
		event.preventDefault();
		back();
	}}
	bind:this={element}
	class="demo-stack-screen @container/panel absolute inset-0 z-20 flex min-w-0 flex-col bg-surface @5xl:static @5xl:z-auto @5xl:w-[28rem] @5xl:shrink-0 @5xl:border-l @5xl:border-border"
	aria-label="Sidebar"
>
	<Tabs.Root bind:value={tab} class="min-h-0 flex-1 flex-col gap-0">
		<header class="flex h-12 shrink-0 items-center justify-between gap-1 border-b border-border px-2 @max-3xl/demo:hidden">
			<!-- Narrow panels show the tab labels without icons; the phone frame also scrolls them sideways. -->
			<Tabs.List variant="line" aria-label="Sidebar" class="no-scrollbar h-full min-w-0 justify-start gap-0 overflow-x-auto p-0 @max-[32rem]/panel:[&_svg]:hidden">
				<Tabs.Trigger value="files" class={tabTrigger}><FileTextIcon />Files</Tabs.Trigger>
				<Tabs.Trigger value="preview" class={tabTrigger}><GlobeIcon />Preview</Tabs.Trigger>
				<Tabs.Trigger value="context" class={tabTrigger}><TreeStructureIcon />Context</Tabs.Trigger>
				<Tabs.Trigger value="todo" class={tabTrigger}>
					<ListChecksIcon />Todo
					{#if count}<Badge class="h-4 min-w-4 rounded-full bg-primary-solid px-1 text-[0.625rem] text-primary-foreground tabular-nums">{count}</Badge>{/if}
				</Tabs.Trigger>
				<Tabs.Trigger value="search" class={tabTrigger}><MagnifyingGlassIcon />Search</Tabs.Trigger>
				<Tabs.Trigger value="git" class={tabTrigger}><GitBranchIcon />Git</Tabs.Trigger>
			</Tabs.List>
			<button type="button" class={iconButton} aria-label="Close" title="Close" onclick={onclose}><XIcon class="size-4" /></button>
		</header>
		<div class="hidden @max-3xl/demo:block">
			<DemoStackHeader title={stack?.title ?? (tab === 'files' && file ? file : tabTitles[tab])} subtitle={stack ? stack.subtitle : tab === 'context' ? contextSubtitle : bot.name} onback={back} />
		</div>

		{#if tab === 'preview'}<DemoPreviewTab bind:url={preview} />{/if}

		<DemoFilesTab files={bot.files} {file} {recent} onopen={onopenfile} onclose={() => (file = null)} onclear={() => (recent = [])} />

		<Tabs.Content value="context" class="flex min-h-0 flex-col">
			{#if !context.length}
				{@render empty(TreeStructureIcon, 'No message selected', "Choose View context from a message's menu.")}
			{:else}
				<DemoBotContextList {bot} messages={context} selected={selected!} {onjump} onopenfile={onopenfile} />
			{/if}
		</Tabs.Content>

		<Tabs.Content value="todo" class="flex min-h-0 flex-col">
			<div class="demo-scroll min-h-0 flex-1 overflow-auto p-3">
				{#if openTodos.length}
					<ul class="space-y-2">{#each openTodos as entry (entry.id)}{@render todoCard(entry)}{/each}</ul>
				{:else}
					{@render empty(CheckCircleIcon, 'No todos')}
				{/if}
				{#if resolvedTodos.length}
					<Collapsible.Root class="mt-4">
						<Collapsible.Trigger class="group flex items-center gap-1 rounded-sm px-1 text-xs font-medium text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary">
							<CaretRightIcon class="size-3 transition-transform group-data-[state=open]:rotate-90" />Recently resolved {resolvedTodos.length}
						</Collapsible.Trigger>
						<Collapsible.Content>
							<ul class="mt-2 space-y-2">{#each resolvedTodos as entry (entry.id)}{@render todoCard(entry)}{/each}</ul>
						</Collapsible.Content>
					</Collapsible.Root>
				{/if}
			</div>
		</Tabs.Content>

		<Tabs.Content value="search" class="flex min-h-0 flex-col">
			<div class="shrink-0 border-b border-border p-3">
				<div class="relative">
					<MagnifyingGlassIcon class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
					<input
						type="search"
						bind:this={field}
						class="h-8 w-full rounded-md border border-border bg-transparent pr-8 pl-8 text-sm outline-none placeholder:text-muted-foreground focus-visible:border-primary [&::-webkit-search-cancel-button]:hidden"
						placeholder="Search this chat"
						aria-label="Search messages"
						bind:value={query}
						onkeydown={(event) => { if (event.key === 'Escape' && query) { event.preventDefault(); query = ''; } }}
					/>
					{#if query}
						<button type="button" class="absolute top-1/2 right-1.5 -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground" aria-label="Clear" onclick={() => (query = '')}><XIcon class="size-3.5" /></button>
					{/if}
				</div>
			</div>
			<div class="demo-scroll min-h-0 flex-1 overflow-auto p-3">
				{#if !query.trim()}
					<p class="px-2 py-6 text-center text-sm text-muted-foreground">Find messages from you and this Bot</p>
				{:else if !results.length}
					{@render empty(MagnifyingGlassIcon, 'No matching messages')}
				{:else}
					<ul class="space-y-1" aria-label="Search results">
						{#each results as entry (entry.id)}
							<li>
								<button type="button" aria-current={entry.id === searchSelected ? 'true' : undefined} class="flex w-full gap-2.5 rounded-xl p-2 text-left hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary aria-current:bg-muted" onclick={() => { searchSelected = entry.id; onjump(entry.id); }}>
									{@render avatar(entry, 'size-5')}
									<span class="min-w-0 flex-1">
										<span class="flex items-center gap-1.5 text-xs text-muted-foreground">
											<span class="truncate font-medium text-heading">{entry.kind === 'user' ? 'You' : bot.name}</span>
											<span class="ml-auto shrink-0 tabular-nums">{entry.time}</span>
										</span>
										<span class="line-clamp-3 text-[13px] leading-5 [overflow-wrap:anywhere]">{#each highlight(plainText(entry)) as part}{#if part.match}<mark class="rounded-sm bg-primary/15 px-0.5 text-foreground">{part.text}</mark>{:else}{part.text}{/if}{/each}</span>
									</span>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		</Tabs.Content>

		<Tabs.Content value="git" class="flex min-h-0 flex-col">
			{#if workspace}
				{#key workspace.id}
					<DemoGitChanges bind:this={git} git={workspace.git} hint="↑↓ select · Enter open · Esc back">
						{#snippet heading()}
								<Select.Root type="single" value={workspace.id} onValueChange={(value) => (workspaceId = value)}>
									<Select.Trigger class="-ml-2 h-8! min-w-0 flex-1 rounded-full border-0 px-2 text-sm hover:bg-muted" title={workspace.path} aria-label={`Workspace: ${workspace.name}`}>
										<FolderIcon class="size-4 text-muted-foreground" />
										<span class="min-w-0 flex-1 truncate text-left font-medium text-heading">{workspace.name}</span>
										<span class="text-xs text-muted-foreground tabular-nums">{workspace.git.changes.length}</span>
									</Select.Trigger>
									<Select.Content align="start" class="w-72 rounded-lg p-1">
										{#each workspaces as item (item.id)}
											<Select.Item value={item.id} label={item.name} class="rounded-sm py-2 text-sm">{@render workspaceLabel(item)}</Select.Item>
										{/each}
									</Select.Content>
								</Select.Root>
						{/snippet}
					</DemoGitChanges>
				{/key}
			{:else}
				<section class="demo-scroll min-h-0 flex-1 overflow-auto p-3" aria-label="Choose a workspace">
					<h2 class="px-2 font-medium text-heading">Choose a workspace</h2>
					<p class="px-2 pt-0.5 pb-2 text-xs text-muted-foreground">Choose a workspace to explore its Git changes.</p>
					<ul>
						{#each workspaces as item (item.id)}
							<li>
								<button type="button" title={item.path} class="flex w-full items-center gap-3 rounded-lg p-2 text-left text-sm hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary" onclick={() => (workspaceId = item.id)}>
									<span class="grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground"><FolderSimpleIcon class="size-5" weight="duotone" /></span>
									{@render workspaceLabel(item)}
								</button>
							</li>
						{/each}
					</ul>
				</section>
			{/if}
		</Tabs.Content>
	</Tabs.Root>
</aside>

<style>
	.no-scrollbar {
		scrollbar-width: none;
	}
	.no-scrollbar::-webkit-scrollbar {
		display: none;
	}
</style>
