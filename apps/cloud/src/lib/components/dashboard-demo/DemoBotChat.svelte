<script lang="ts">
	import { tick } from 'svelte';
	import ArrowBendUpLeftIcon from 'phosphor-svelte/lib/ArrowBendUpLeftIcon';
	import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon';
	import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon';
	import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon';
	import EyeSlashIcon from 'phosphor-svelte/lib/EyeSlashIcon';
	import PlusIcon from 'phosphor-svelte/lib/PlusIcon';
	import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon';
	import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon';
	import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon';
	import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon';
	import XIcon from 'phosphor-svelte/lib/XIcon';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { cn } from '$lib/utils';
	import BotAvatarMark from '$lib/components/bot-story/BotAvatarMark.svelte';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import DemoBotPanel, { type PanelTab } from './DemoBotPanel.svelte';
	import DemoInline from './DemoInline.svelte';
	import DemoActionSheet from './DemoActionSheet.svelte';
	import PaperPlaneTiltIcon from 'phosphor-svelte/lib/PaperPlaneTiltIcon';
	import type { DemoBot, DemoBotEntry, DemoTodo, DemoWorkspace } from './mockData';
	import { plainText } from './text';

	let { bot, workspaces, phone, onopensidebar, onopenbrowser, ontodochange, onsend }: {
		bot: DemoBot;
		workspaces: DemoWorkspace[];
		/** Whether the demo uses the phone layout, where localhost links open a browser tab. */
		phone: boolean;
		onopensidebar: () => void;
		onopenbrowser: (url: string) => void;
		onsend: (text: string, quote?: string) => void;
		ontodochange: (entry: Extract<DemoBotEntry, { kind: 'bot' }>, todo: DemoTodo) => void;
	} = $props();

	let scroller: HTMLDivElement;
	let panelElement = $state<HTMLElement>();
	let panelOpen = $state(false);
	let tab = $state<PanelTab>('files');
	let file = $state<string | null>(null);
	let preview = $state<string | null>(null);
	let menu = $state(false);
	let actionsButton = $state<HTMLButtonElement>();
	let selected = $state<string | null>(null);
	// The chat is recreated for another Bot.
	// svelte-ignore state_referenced_locally
	let recent = $state([...bot.recentFiles]);
	let query = $state('');
	let searchSelected = $state<string | null>(null);
	let draft = $state('');
	let composer: HTMLInputElement;
	let workspaceId = $state<string | null>(null);
	let quote = $state<DemoBotEntry | null>(null);
	let highlighted = $state<string | null>(null);
	let highlightTimer: ReturnType<typeof setTimeout> | undefined;

	const count = $derived(bot.entries.filter((entry) => entry.kind === 'bot' && entry.todo === 'open').length);
	const quoteSummary = $derived(quote ? plainText(quote) : '');

	$effect(() => {
		scroller.scrollTop = scroller.scrollHeight;
		return () => clearTimeout(highlightTimer);
	});

	function speaker(entry: DemoBotEntry): string {
		return entry.kind === 'user' ? 'You' : bot.name;
	}

	function openPanel(next: PanelTab) {
		tab = next;
		panelOpen = true;
	}

	function openFile(path: string) {
		file = path;
		recent = [path, ...recent.filter((item) => item !== path)];
		openPanel('files');
	}

	function openPreview(url: string) {
		if (phone) return onopenbrowser(url);
		preview = url;
		openPanel('preview');
	}

	function viewContext(id: string) {
		selected = id;
		openPanel('context');
	}

	// On narrow frames the panel covers the feed, so acting on a message closes it first.
	async function closeIfCovering() {
		if (panelElement && getComputedStyle(panelElement).position === 'absolute') {
			panelOpen = false;
			await tick();
		}
	}

	/** Scrolls a message to the middle of the feed and rings it briefly. The page itself does not scroll. */
	async function jump(id: string) {
		await closeIfCovering();
		const row = scroller.querySelector<HTMLElement>(`[data-entry-id="${id}"]`);
		if (!row) return;
		scroller.scrollTo({ top: row.offsetTop - (scroller.clientHeight - row.offsetHeight) / 2, behavior: 'smooth' });
		highlighted = id;
		clearTimeout(highlightTimer);
		highlightTimer = setTimeout(() => (highlighted = null), 1500);
	}

	async function addQuote(entry: DemoBotEntry) {
		await closeIfCovering();
		quote = entry;
		await tick();
		composer.focus({ preventScroll: true });
		scroller.scrollTop = scroller.scrollHeight;
	}

	async function send(text: string, id?: string) {
		onsend(text, id);
		await tick();
		scroller.scrollTop = scroller.scrollHeight;
	}
</script>

{#snippet badge(corner = false)}
	{#if count > 0}<Badge class={cn('h-4 min-w-4 rounded-full bg-primary-solid px-1 text-[0.625rem] text-primary-foreground tabular-nums', corner && 'pointer-events-none absolute -top-1 -right-1')}>{count}</Badge>{/if}
{/snippet}

{#snippet menuButton(entry: DemoBotEntry)}
	<DropdownMenu.Root>
		<DropdownMenu.Trigger
			class="flex size-7 shrink-0 items-center justify-center self-end rounded-md text-muted-foreground opacity-0 transition-opacity hover:bg-muted focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-primary group-hover/entry:opacity-100 data-[state=open]:opacity-100 pointer-coarse:opacity-100"
			aria-label="Message actions"
		>
			<DotsThreeIcon class="size-4" weight="bold" />
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="start" class="min-w-40 rounded-lg p-1 shadow-none">
			<DropdownMenu.Item class="gap-1.5 rounded-sm py-1 pl-1.5 text-sm" onSelect={() => void addQuote(entry)}><ArrowBendUpLeftIcon />Reply</DropdownMenu.Item>
			<DropdownMenu.Item class="gap-1.5 rounded-sm py-1 pl-1.5 text-sm" onSelect={() => viewContext(entry.id)}><TreeStructureIcon />View context</DropdownMenu.Item>
			{#if entry.kind === 'bot' && entry.todo === 'open'}
				<DropdownMenu.Separator class="my-1" />
				<DropdownMenu.Item class="gap-1.5 rounded-sm py-1 pl-1.5 text-sm" onSelect={() => ontodochange(entry, { by: 'user', at: 'Just now' })}><EyeSlashIcon />Ignore</DropdownMenu.Item>
			{/if}
		</DropdownMenu.Content>
	</DropdownMenu.Root>
{/snippet}

<div class="relative flex min-w-0 flex-1 flex-col">
	<div class="pointer-events-none absolute inset-x-0 top-0 z-10 flex h-12 items-center gap-2 px-3 @3xl:px-4 @max-3xl/demo:h-16 @max-3xl/demo:bg-linear-to-b @max-3xl/demo:from-surface @max-3xl/demo:from-70% @max-3xl/demo:to-surface/0 @max-3xl/demo:pt-1">
		<button type="button" class="demo-glass pointer-events-auto grid size-10 shrink-0 place-items-center rounded-full @3xl:hidden" aria-label="Open sidebar" onclick={onopensidebar}>
			<CaretLeftIcon class="size-[18px]" />
		</button>
		<div class="demo-title-pill pointer-events-auto flex h-8 min-w-0 items-center rounded-full bg-popover px-1 ring-1 ring-foreground/10 @max-3xl/demo:h-10 @max-3xl/demo:ring-0">
			<span class="mx-1 flex min-w-0 max-w-80 items-center gap-1.5 px-2 py-0.5 text-sm text-heading @max-3xl/demo:text-[15px] @max-3xl/demo:font-semibold">
				<span class="size-5 shrink-0"><BotAvatarMark shape={bot.shape} color={bot.color} /></span>
				<span class="min-w-0 truncate">{bot.name}</span>
			</span>
		</div>
		<span class="flex-1"></span>
		<button type="button" class="demo-glass pointer-events-auto grid size-10 shrink-0 place-items-center rounded-full @3xl:hidden" bind:this={actionsButton} aria-label="Chat actions" onclick={() => (menu = true)}>
			<DotsThreeIcon class="size-5" weight="bold" />
		</button>
		<span class="pointer-events-auto flex shrink-0 items-center @max-3xl/demo:hidden">
			<button
				type="button"
				class={cn('relative flex size-8 items-center justify-center rounded-full ring-1 ring-foreground/10 hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary', panelOpen ? 'bg-muted text-heading' : 'bg-popover')}
				aria-label={count ? `Sidebar, ${count} ${count === 1 ? 'todo' : 'todos'}` : 'Sidebar'}
				aria-pressed={panelOpen}
				title="Sidebar"
				onclick={() => (panelOpen = !panelOpen)}
			>
				<SidebarSimpleIcon class="size-4 -scale-x-100" />{@render badge(true)}
			</button>
		</span>
	</div>

	<div bind:this={scroller} class="demo-scroll relative min-h-0 flex-1 overflow-y-auto px-4 md:px-8" role="log" aria-label="Messages">
		<ol class="mx-auto flex w-full max-w-3xl flex-col gap-5 pt-14 pb-5 @3xl:pt-12 @max-3xl/demo:pt-16">
			{#each bot.entries as entry (entry.id)}
				{@const ringed = highlighted === entry.id && 'ring-2 ring-primary/50'}
				<li data-entry-id={entry.id}>
					{#if entry.kind === 'notice'}
						<p class="flex items-center justify-center gap-2 text-center text-xs text-muted-foreground">
							<WarningCircleIcon class="size-4 shrink-0" />{entry.text}<span class="tabular-nums">{entry.time}</span>
						</p>
					{:else}
						{@const user = entry.kind === 'user'}
						{@const quoted = user && entry.quote ? bot.entries.find((item) => item.id === entry.quote) : undefined}
						<div class={cn('group/entry flex gap-1', user ? 'justify-end' : 'justify-start')}>
							{#if user}{@render menuButton(entry)}{/if}
							<div class="max-w-[90%] min-w-0 space-y-2">
								{#if entry.kind === 'bot'}
									<div class={cn('rounded-2xl bg-muted px-4 py-3 text-sm leading-6 break-words transition-shadow', ringed)}>
										{#each entry.blocks as block}
											{#if block.kind === 'p'}
												<p class="my-2 first:mt-0 last:mb-0"><DemoInline text={block.text} code="bg-background" onlink={openFile} onpreview={openPreview} /></p>
											{:else}
												<ul class="my-2 list-disc pl-5 first:mt-0 last:mb-0">
													{#each block.items as item}<li class="my-0.5"><DemoInline text={item} code="bg-background" onlink={openFile} onpreview={openPreview} /></li>{/each}
												</ul>
											{/if}
										{/each}
									</div>
								{:else}
									<div class={cn('rounded-2xl border border-border bg-background px-4 py-3 text-sm leading-6 transition-shadow', ringed)}>
										{#if quoted}
											<button type="button" class="mb-1.5 flex w-full max-w-sm min-w-0 items-center gap-1 border-l-2 border-border pl-2.5 text-left text-[13px] leading-5 text-muted-foreground hover:border-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary" aria-label={`Quote from ${speaker(quoted)}: ${plainText(quoted)}`} title={`${speaker(quoted)}: ${plainText(quoted)}`} onclick={() => jump(quoted.id)}>
												<span class="min-w-0 truncate">{plainText(quoted)}</span>
											</button>
										{/if}
										<p class="whitespace-pre-wrap [overflow-wrap:anywhere]">{entry.text}</p>
									</div>
								{/if}
								<div class={cn('-mt-1 flex items-center gap-2 text-xs text-muted-foreground', user ? 'justify-end' : 'justify-start')}>
									{#if entry.kind === 'user' && entry.failed}
										<span class="flex items-center gap-2 text-destructive">Message failed to send<span class="px-2 font-medium">Retry</span></span>
									{/if}
									<span class="px-1 tabular-nums">{entry.time}</span>
								</div>
							</div>
							{#if entry.kind === 'bot' && entry.todo === 'open'}
								<button type="button" class="grid size-7 shrink-0 place-items-center self-end rounded-md hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary" aria-label="Todo" title="Todo" onclick={() => openPanel('todo')}>
									<span class="size-2 rounded-full bg-primary"></span>
								</button>
							{/if}
							{#if !user}{@render menuButton(entry)}{/if}
						</div>
					{/if}
				</li>
			{/each}
		</ol>
	</div>

	<div class="relative bg-surface px-4 pb-2 md:px-8 md:pb-5">
		<div class="pointer-events-none absolute inset-x-0 bottom-full h-6 bg-linear-to-t from-surface to-surface/0"></div>
		<div class="mx-auto w-full max-w-3xl">
			{#if bot.working}
				<p class="mb-2 flex items-start gap-2 px-2 text-sm leading-5 text-muted-foreground" role="status">
					<span class="size-5 shrink-0"><BotAvatarMark shape={bot.shape} color={bot.color} /></span>
					<span class="demo-shimmer line-clamp-2 min-w-0">{bot.working}</span>
				</p>
			{/if}
			{#if quote}
				<div class="mb-1.5 flex h-8 items-center gap-2 rounded-lg bg-muted pr-1 pl-2.5 text-[13px] text-muted-foreground" title={`${speaker(quote)}: ${quoteSummary}`}>
					<QuotesIcon class="size-3.5 shrink-0 opacity-70" />
					<span class="min-w-0 flex-1 truncate">{quoteSummary}</span>
					<button type="button" class="grid size-6 shrink-0 place-items-center rounded-md hover:bg-background hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary" aria-label={`Remove quote: ${quoteSummary}`} title="Remove" onclick={() => (quote = null)}>
						<XIcon class="size-3" />
					</button>
				</div>
			{/if}
			<form class="flex items-center rounded-3xl border border-border bg-background focus-within:border-primary @max-3xl/demo:min-h-12 @max-3xl/demo:rounded-full @max-3xl/demo:pr-1" onsubmit={(event) => {
				event.preventDefault();
				if (!draft.trim()) return;
				void send(draft.trim(), quote?.id);
				draft = '';
				quote = null;
			}}>
				<span class="flex size-7 shrink-0 items-center justify-center pl-1 text-foreground @max-3xl/demo:ml-2"><PlusIcon class="size-4" /></span>
				<input bind:this={composer} bind:value={draft} class="min-w-0 flex-1 bg-transparent py-2 pl-1.5 text-[13.5px] outline-none placeholder:text-muted-foreground" placeholder="Message" aria-label="Message" />
				<div class="shrink-0 p-1">
					<button type="submit" disabled={!draft.trim()} aria-label="Send" class="inline-flex h-7 w-7 items-center justify-center gap-1.5 rounded-full bg-primary-solid text-xs font-semibold text-primary-foreground disabled:bg-border disabled:text-inactive @md:w-auto @md:px-3 @max-3xl/demo:size-10! @max-3xl/demo:px-0! @max-3xl/demo:disabled:bg-primary/35 @max-3xl/demo:disabled:text-primary-foreground">
						<span class="hidden @md:inline @max-3xl/demo:hidden!">Send</span><ArrowUpIcon class="size-3.5 @max-3xl/demo:hidden" /><PaperPlaneTiltIcon class="hidden size-5 @max-3xl/demo:block" />
					</button>
				</div>
			</form>
		</div>
	</div>
</div>

{#if panelOpen}
	<DemoBotPanel {phone} bind:element={panelElement} bind:tab bind:file bind:preview bind:selected bind:recent bind:query bind:searchSelected bind:workspaceId {bot} {workspaces} {count} onopenfile={openFile} onjump={jump} onquote={addQuote} onreply={send} {ontodochange} onclose={() => (panelOpen = false)} />
{/if}
{#if menu}
	<DemoActionSheet title={bot.name} subtitle="Bot" onclose={() => { menu = false; actionsButton?.focus({ preventScroll: true }); }} rows={[
		{ label: 'Files', detail: String(recent.length), onselect: () => { menu = false; openPanel('files'); } },
		{ label: 'Todo', detail: String(count), onselect: () => { menu = false; openPanel('todo'); } },
		{ label: 'Search', onselect: () => { menu = false; openPanel('search'); } },
		{ label: 'Git', onselect: () => { menu = false; openPanel('git'); } }
	]} />
{/if}

<style>
	/* The Dashboard's text shimmer for the Bot's current work. */
	.demo-shimmer {
		background: linear-gradient(90deg, currentColor 40%, color-mix(in srgb, currentColor 25%, transparent) 50%, currentColor 60%) 0 0 / 300% 100%;
		background-clip: text;
		-webkit-text-fill-color: transparent;
		animation: demo-shimmer 2s linear infinite;
	}
	@keyframes demo-shimmer {
		from { background-position: 100% 0; }
		to { background-position: 0 0; }
	}
	@media (prefers-reduced-motion: reduce) {
		.demo-shimmer { animation: none; -webkit-text-fill-color: currentColor; }
	}
</style>
