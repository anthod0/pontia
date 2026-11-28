<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import * as Tabs from '$lib/components/ui/tabs';
	import StoryTabAvatar from './bot-story/StoryTabAvatar.svelte';
	import StoryStage from './bot-story/StoryStage.svelte';
	import { reveal } from '$lib/reveal';
	import { stories } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	const STEP_MS = 1000;
	const HOLD_MS = 2200;
	const lastStep = (index: number) => stories[index].steps - 1;

	const reducedMotion = new MediaQuery('(prefers-reduced-motion: reduce)', true);
	let visible = $state(false);
	let index = $state(0);
	let step = $state(0);
	// Once the visitor picks a Bot, the demo replays that Bot instead of moving on to the next.
	let picked = false;
	const animate = $derived(visible && !reducedMotion.current);
	let list = $state<HTMLElement | null>(null);
	const story = $derived(stories[index]);

	// On narrow screens the avatar row scrolls sideways; keep the selected Bot in view without moving the page.
	$effect(() => {
		const tab = list?.children[index] as HTMLElement | undefined;
		if (!list || !tab || list.scrollWidth <= list.clientWidth) return;
		const left = tab.offsetLeft - (list.clientWidth - tab.offsetWidth) / 2;
		list.scrollTo({ left, behavior: animate ? 'smooth' : 'auto' });
	});

	$effect(() => {
		if (!animate) return;
		const atEnd = step === lastStep(index);
		const timer = setTimeout(() => {
			if (atEnd) {
				if (!picked) index = (index + 1) % stories.length;
				step = 0;
			} else {
				step += 1;
			}
		}, atEnd ? HOLD_MS : STEP_MS);
		return () => clearTimeout(timer);
	});

	function select(next: number) {
		picked = true;
		index = next;
		step = 0;
	}
</script>

<section id="bots" class="page-width section-block section" aria-labelledby="bots-title" {@attach reveal(() => { visible = true; })}>
	<div class="intro reveal">
		<h2 id="bots-title" class="section-title">{m.bots_title()}</h2>
		<p>{m.bots_intro()}</p>
	</div>

	<Tabs.Root class="block" value={story.name} onValueChange={(name) => select(stories.findIndex((s) => s.name === name))}>
		<Tabs.List bind:ref={list} class="tabs reveal" style="--i: 1" aria-label={m.bots_pick()}>
			{#each stories as s (s.name)}
				<Tabs.Trigger value={s.name} class="tab focus-visible:border-transparent focus-visible:ring-0">
					<span class="avatar"><StoryTabAvatar shape={s.shape} color={s.color} working={s === story} /></span>
					<b>{s.name}</b>
				</Tabs.Trigger>
			{/each}
		</Tabs.List>

		{#each stories as s, storyIndex (s.name)}
			<Tabs.Content value={s.name} class="reveal" style="--i: 2">
				{#if storyIndex === index}
					<div class="caption">
						<h3><span class="caption-name">{s.name}</span>{s.label()}</h3>
						<p>{s.caption()}</p>
					</div>
					<StoryStage story={s} step={animate ? step : lastStep(index)} />
				{/if}
			</Tabs.Content>
		{/each}
	</Tabs.Root>
</section>

<style>
	.intro { text-align: center; }
	.intro p { max-width: 560px; margin: 14px auto 0; font-size: 15px; line-height: 1.75; color: var(--muted-foreground); text-wrap: balance; }
	/* Chinese copy wraps at punctuation and spaces; Japanese wraps between phrases. */
	:global(:root:lang(zh-Hans)) :is(.intro :is(h2, p), .caption) { word-break: keep-all; }
	:global(:root:lang(ja)) :is(.intro :is(h2, p), .caption) { word-break: auto-phrase; }

	.section :global(.tabs) { display: flex; align-items: flex-start; width: auto; height: auto; padding: 0; background: none; justify-content: center; gap: 0; margin: 44px 0 20px; }
	.section :global(.tab) { display: flex; flex: 0 1 112px; min-width: 0; height: auto; white-space: nowrap; border-radius: 0; background: none; box-shadow: none; flex-direction: column; align-items: center; gap: 4px; padding: 10px 4px 12px; border: 0; border-bottom: 2px solid transparent; font: inherit; color: var(--muted-foreground); cursor: pointer; transition: border-color 140ms, color 140ms; }
	.section :global(.tab:focus-visible) { outline: 2px solid var(--primary); outline-offset: 2px; }
	.section :global(.tab[data-state='active']) { border-color: var(--primary); color: var(--heading); }
	.avatar { width: 56px; height: 56px; margin-bottom: 4px; transition: scale 140ms var(--ease-out), opacity 140ms, filter 140ms; }
	.section :global(.tab:not([data-state='active']) .avatar) { opacity: 0.7; filter: saturate(0.55); scale: 0.86; }
	.section :global(.tab:hover .avatar) { opacity: 1; filter: none; }

	.section :global(.tab b) { font-size: 14px; font-weight: 500; }

	.caption { margin-bottom: 16px; text-align: center; animation: fade 160ms var(--ease-out); }
	.caption h3 { margin: 0; font-size: 16px; font-weight: 600; color: var(--heading); }
	.caption-name { display: none; }
	.caption p { margin: 4px 0 0; font-size: 14px; line-height: 1.6; color: var(--muted-foreground); text-wrap: balance; }

	@keyframes fade { from { opacity: 0; } }
	@media (prefers-reduced-motion: reduce) { .caption { animation: none; } }

	/* Narrow screens: a row of avatars that scrolls sideways. The selected Bot's name leads the caption. */
	@media (max-width: 760px) {
		.section :global(.tabs) { justify-content: flex-start; gap: 2px; margin: 28px -20px 14px; padding: 4px 20px 0; overflow-x: auto; scroll-snap-type: x proximity; scrollbar-width: none; }
		.section :global(.tabs::-webkit-scrollbar) { display: none; }
		.section :global(.tab) { flex: none; width: 56px; padding: 6px 4px 8px; scroll-snap-align: center; }
		.section :global(.tab b) { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
		.avatar { width: 44px; height: 44px; margin-bottom: 0; }
		.caption { text-align: left; }
		.caption-name { display: inline; color: var(--primary); }
		.caption-name::after { content: '·'; margin-inline: 0.4em; color: var(--muted-foreground); }
		.caption h3 { font-size: 15px; }
		.caption p { font-size: 13px; text-wrap: pretty; }
	}
</style>
