<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';
	import CaretUpIcon from 'phosphor-svelte/lib/CaretUpIcon';
	import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon';
	import CrosshairSimpleIcon from 'phosphor-svelte/lib/CrosshairSimpleIcon';
	import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon';
	import BotAvatarMark from '$lib/components/bot-story/BotAvatarMark.svelte';
	import { cn } from '$lib/utils';
	import type { DemoBot, DemoBotEntry } from './mockData';
	import DemoInline from './DemoInline.svelte';
	import { plainText } from './text';

	type Message = Exclude<DemoBotEntry, { kind: 'notice' }>;
	let { bot, messages, selected, onjump, onopenfile }: {
		bot: DemoBot;
		messages: Message[];
		selected: string;
		onjump: (id: string) => void;
		onopenfile: (path: string) => void;
	} = $props();

	let scroller = $state<HTMLElement>();
	let open = $state<Set<string>>(new Set());
	let clamped = $state<Set<string>>(new Set());
	let flashed = $state<string | null>(null);
	let flashTimer: ReturnType<typeof setTimeout> | null = null;
	const bodies = new Map<string, HTMLElement>();

	onDestroy(() => { if (flashTimer) clearTimeout(flashTimer); });

	$effect(() => reset(selected, messages));

	function reset(_selected: string, _messages: Message[]) {
		open = new Set();
		clamped = new Set();
		void tick().then(() => {
			for (const [id, node] of bodies) measure(id, node);
			requestAnimationFrame(() => scroller?.querySelector(`[data-demo-context-message="${CSS.escape(selected)}"]`)?.scrollIntoView({ block: 'center' }));
		});
	}

	function observeBody(id: string, node: HTMLElement) {
		bodies.set(id, node);
		const observer = new ResizeObserver(() => measure(id, node));
		observer.observe(node);
		void tick().then(() => measure(id, node));
		return () => { observer.disconnect(); bodies.delete(id); };
	}

	function measure(id: string, node: HTMLElement) {
		const line = Number.parseFloat(getComputedStyle(node).lineHeight);
		const value = id !== selected && node.scrollHeight > line * 5 + 1;
		if (clamped.has(id) === value) return;
		const next = new Set(clamped);
		if (value) next.add(id); else next.delete(id);
		clamped = next;
	}

	function toggle(id: string, value: boolean) {
		const next = new Set(open);
		if (value) next.add(id); else next.delete(id);
		open = next;
		if (!value) void tick().then(() => scroller?.querySelector(`[data-demo-context-message="${CSS.escape(id)}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }));
	}

	function interactive(target: EventTarget | null) {
		return target instanceof Element && Boolean(target.closest('button, a, [role="button"]'));
	}

	function clickItem(event: MouseEvent, id: string) {
		if (clamped.has(id) && !open.has(id) && !interactive(event.target) && window.getSelection()?.isCollapsed !== false) toggle(id, true);
	}

	async function quoteJump(id: string) {
		const target = messages.find((entry) => entry.id === id);
		if (!target) { onjump(id); return; }
		toggle(id, true);
		await tick();
		scroller?.querySelector(`[data-demo-context-message="${CSS.escape(id)}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
		flashed = id;
		if (flashTimer) clearTimeout(flashTimer);
		flashTimer = setTimeout(() => { flashed = null; flashTimer = null; }, 1800);
	}
</script>

<div bind:this={scroller} class="demo-scroll min-h-0 flex-1 overflow-auto overscroll-contain" data-demo-context-list>
	<div class="sticky top-0 z-20 bg-surface px-2.5 @max-3xl/demo:hidden">
		<div class="flex items-center gap-2 px-3 pt-3 pb-2 text-[13px] text-muted-foreground"><TreeStructureIcon class="size-4" />{messages.length} messages · {messages[0]?.time} – {messages.at(-1)?.time}</div>
	</div>
	<ol class="m-0 list-none p-0 @max-3xl/demo:pt-1">
		{#each messages as entry (entry.id)}
			{@const current = entry.id === selected}
			{@const user = entry.kind === 'user'}
			{@const expanded = open.has(entry.id)}
			{@const isClamped = clamped.has(entry.id)}
			<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
			<li
				data-demo-context-message={entry.id}
				aria-current={current ? 'true' : undefined}
				class={cn(
					'group/context relative px-[22px] py-3.5 transition-colors @max-3xl/demo:px-7',
					current && 'bg-selected before:absolute before:inset-y-0 before:left-0 before:w-[3px] before:bg-primary',
					isClamped && !expanded && 'cursor-pointer hover:bg-muted/75 dark:hover:bg-muted/60',
					flashed === entry.id && 'demo-context-flash'
				)}
				onclick={(event) => clickItem(event, entry.id)}
			>
				{#if entry !== messages.at(-1)}<span aria-hidden="true" class="absolute top-11 -bottom-2 left-[33.5px] w-px bg-border @max-3xl/demo:left-[39.5px]"></span>{/if}
				<article class="relative grid grid-cols-[1.5rem_minmax(0,1fr)] gap-3" aria-label={`${user ? 'You' : bot.name} ${entry.time}`}>
					<span class="relative z-10 grid size-6 place-items-center rounded-full bg-surface">
						{#if user}<span aria-hidden="true" class="grid size-6 place-items-center rounded-full bg-border text-[11px] font-semibold text-heading">Y</span>
						{:else}<BotAvatarMark shape={bot.shape} color={bot.color} />{/if}
					</span>
					<div class="min-w-0">
						<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
						<header class={cn('flex min-h-6 items-center gap-2 text-sm', expanded && isClamped && 'cursor-pointer')} onclick={(event) => { if (expanded && isClamped && !interactive(event.target)) { event.stopPropagation(); toggle(entry.id, false); } }}>
							<span class="font-semibold text-heading">{user ? 'You' : bot.name}</span><span class="text-[13px] text-muted-foreground tabular-nums">{entry.time}</span>
							{#if current}<span class="inline-flex h-5 items-center rounded-md bg-primary/14 px-1.5 text-xs font-medium text-primary">This message</span>{/if}
							{#if entry.kind === 'bot' && entry.todo === 'open'}<span class="inline-flex h-5 items-center rounded-md bg-primary/14 px-1.5 text-xs font-medium text-primary">Todo</span>
							{:else if entry.kind === 'bot' && entry.todo}<span class="inline-flex items-center gap-1 text-xs text-muted-foreground"><CheckCircleIcon class="size-3" />Done</span>{/if}
							{#if entry.kind === 'user' && entry.failed}<span class="inline-flex h-5 items-center rounded-md bg-destructive/12 px-1.5 text-xs font-medium text-destructive">Send failed</span>{/if}
							<button type="button" class={cn('ml-auto inline-flex h-7 shrink-0 items-center gap-1 rounded-full px-2.5 text-[13px] text-muted-foreground hover:bg-muted hover:text-heading focus-visible:ring-2 focus-visible:ring-primary @max-3xl/demo:min-h-11 @max-3xl/demo:px-3 @max-3xl/demo:text-sm', current && 'hover:bg-primary/14')} aria-label={`Show ${user ? 'your' : `${bot.name}'s`} message from ${entry.time} in chat`} title="Show in chat" onclick={() => onjump(entry.id)}><CrosshairSimpleIcon class="size-[15px]" />Show</button>
						</header>
						<div id={`demo-context-body-${entry.id}`} class={cn('mt-1 text-[15px] leading-[1.75] text-foreground [overflow-wrap:anywhere] @max-3xl/demo:text-base @max-3xl/demo:leading-[1.7]', isClamped && !expanded && 'demo-context-clamped')} {@attach node => observeBody(entry.id, node)}>
							{#if entry.kind === 'user'}
								{#if entry.quote}
									{@const quote = messages.find((message) => message.id === entry.quote) ?? bot.entries.find((message) => message.id === entry.quote)}
									{#if quote}<button type="button" class="mb-1.5 flex w-full max-w-sm min-w-0 items-center border-l-2 border-border pl-2.5 text-left text-[13px] leading-5 text-muted-foreground hover:border-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary" onclick={() => void quoteJump(quote.id)}><span class="truncate">{plainText(quote)}</span></button>{/if}
								{/if}
								<p class="whitespace-pre-wrap">{entry.text}</p>
							{:else}
								<div class="context-markdown [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
									{#each entry.blocks as block}
										{#if block.kind === 'p'}<p class="my-2.5"><DemoInline text={block.text} code={current ? 'bg-background' : 'bg-muted'} onlink={onopenfile} /></p>
										{:else}<ul class="my-2.5 list-disc pl-[1.4em] marker:text-muted-foreground">{#each block.items as item}<li class="my-1"><DemoInline text={item} code={current ? 'bg-background' : 'bg-muted'} onlink={onopenfile} /></li>{/each}</ul>{/if}
									{/each}
								</div>
							{/if}
						</div>
						{#if isClamped}<button type="button" class="mt-0.5 inline-flex h-7 items-center gap-1 text-[13px] font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-primary @max-3xl/demo:min-h-11 @max-3xl/demo:text-sm" aria-expanded={expanded} aria-controls={`demo-context-body-${entry.id}`} onclick={() => toggle(entry.id, !expanded)}>{#if expanded}<CaretUpIcon />Show less{:else}<CaretDownIcon />Show more{/if}</button>{/if}
					</div>
				</article>
			</li>
		{/each}
	</ol>
	{#if messages.length === 1}<p class="px-4 pt-4 text-center text-[13px] text-muted-foreground">This message has no quoted or linked messages</p>{/if}
</div>

<style>
	.demo-context-clamped { max-height: 7em; overflow: hidden; mask-image: linear-gradient(to bottom, #000 calc(100% - 1.75em), transparent); }
	.demo-context-flash { animation: demo-context-flash 1.8s ease-out; }
	@keyframes demo-context-flash { 0%, 55% { background: color-mix(in srgb, var(--primary) 16%, transparent); } 100% { background: transparent; } }
</style>
