<script lang="ts">
	import { onDestroy } from 'svelte';
	import BrainIcon from 'phosphor-svelte/lib/BrainIcon';
	import DevicesIcon from 'phosphor-svelte/lib/DevicesIcon';
	import FileArrowDownIcon from 'phosphor-svelte/lib/FileArrowDownIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import PauseIcon from 'phosphor-svelte/lib/PauseIcon';
	import PlayIcon from 'phosphor-svelte/lib/PlayIcon';
	import ArrowsClockwiseIcon from 'phosphor-svelte/lib/ArrowsClockwiseIcon';
	import UserSwitchIcon from 'phosphor-svelte/lib/UserSwitchIcon';
	import * as Tabs from '$lib/components/ui/tabs';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import BotStoryStage from './BotStoryStage.svelte';
	import { reveal } from '$lib/reveal';
	import { stories } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	const STEP_MS = 1900;
	const HOLD_MS = 3600;
	const lastStep = (index: number) => stories[index].lines.length - 1;

	const features = [
		{ icon: UserSwitchIcon, title: m.bots_feature_profiles_title, text: m.bots_feature_profiles_text },
		{ icon: GitBranchIcon, title: m.bots_feature_worktrees_title, text: m.bots_feature_worktrees_text },
		{ icon: BrainIcon, title: m.bots_feature_memory_title, text: m.bots_feature_memory_text },
		{ icon: FileArrowDownIcon, title: m.bots_feature_files_title, text: m.bots_feature_files_text },
		{ icon: DevicesIcon, title: m.bots_feature_devices_title, text: m.bots_feature_devices_text },
		{ icon: ArrowsClockwiseIcon, title: m.bots_feature_restart_title, text: m.bots_feature_restart_text }
	];

	// The prerendered page shows Atlas's final frame, so it reads complete without JavaScript.
	let index = $state(0);
	let step = $state(lastStep(0));
	let playing = $state(false);
	// Once the visitor picks a Bot, the demo replays that Bot instead of moving on to the next.
	let picked = false;
	let timer: ReturnType<typeof setTimeout> | undefined;

	function schedule() {
		clearTimeout(timer);
		if (!playing) return;
		const last = lastStep(index);
		timer = setTimeout(
			() => {
				if (step < last) step += 1;
				else {
					if (!picked) index = (index + 1) % stories.length;
					step = 0;
				}
				schedule();
			},
			step === last ? HOLD_MS : STEP_MS
		);
	}

	function start() {
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		step = 0;
		playing = true;
		schedule();
	}

	function select(next: number) {
		picked = true;
		index = next;
		step = playing ? 0 : lastStep(next);
		schedule();
	}

	function toggle() {
		playing = !playing;
		if (playing && step === lastStep(index)) step = 0;
		schedule();
	}

	onDestroy(() => clearTimeout(timer));
</script>

<section id="bots" class="page-width section-block section" aria-labelledby="bots-title" {@attach reveal(start)}>
	<div class="intro reveal">
		<h2 id="bots-title">{m.bots_title()}</h2>
		<p>{m.bots_intro()}</p>
	</div>

	<Tabs.Root class="block" value={stories[index].name} onValueChange={(name) => select(stories.findIndex((story) => story.name === name))}>
		<Tabs.List class="tabs reveal" style="--i: 1" aria-label={m.bots_pick()}>
			{#each stories as story (story.name)}
				<Tabs.Trigger value={story.name} class="tab focus-visible:border-transparent focus-visible:ring-0">
					<span class="body"><BotAvatarMark shape={story.shape} color={story.color} /></span>
					<b>{story.name}</b>
					<span class="label">{story.label()}</span>
				</Tabs.Trigger>
			{/each}
		</Tabs.List>

		{#each stories as story, storyIndex (story.name)}
			<Tabs.Content value={story.name} class="reveal" style="--i: 2">
				{#if storyIndex === index}
					<div class="caption-row">
						<p class="caption">{story.caption()}</p>
						<button type="button" class="play" onclick={toggle} aria-label={playing ? m.bots_pause() : m.bots_play()}>
							{#if playing}<PauseIcon size={14} weight="fill" />{:else}<PlayIcon size={14} weight="fill" />{/if}
						</button>
					</div>
					<BotStoryStage {story} {step} />
				{/if}
			</Tabs.Content>
		{/each}
	</Tabs.Root>

	<ul class="features reveal" style="--i: 3">
		{#each features as feature (feature.title)}
			{@const Icon = feature.icon}
			<li>
				<Icon size={18} />
				<b>{feature.title()}</b>
				<span>{feature.text()}</span>
			</li>
		{/each}
	</ul>
</section>

<style>
	.intro { text-align: center; }
	.intro p { max-width: 560px; margin: 14px auto 0; font-size: 15px; line-height: 1.75; color: var(--muted-foreground); text-wrap: balance; }
	/* Chinese copy wraps at punctuation and spaces; Japanese wraps between phrases. */
	:global(:root:lang(zh-Hans)) :is(.intro :is(h2, p), .features) { word-break: keep-all; }
	:global(:root:lang(ja)) :is(.intro :is(h2, p), .features, .label) { word-break: auto-phrase; }

	.section :global(.tabs) { display: flex; align-items: flex-start; width: auto; height: auto; padding: 0; background: none; justify-content: center; gap: clamp(12px, 4vw, 56px); margin: 44px 0 24px; }
	.section :global(.tab) { display: flex; flex: none; height: auto; white-space: normal; border-radius: 0; background: none; box-shadow: none; flex-direction: column; align-items: center; gap: 4px; width: 160px; padding: 10px 6px 12px; border: 0; border-bottom: 2px solid transparent; font: inherit; color: var(--muted-foreground); cursor: pointer; transition: border-color 200ms, color 200ms; }
	.section :global(.tab:focus-visible), .play:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
	.section :global(.tab[data-state='active']) { border-color: var(--primary); color: var(--heading); }
	.body { width: 64px; height: 64px; margin-bottom: 6px; transition: scale 200ms var(--ease-out), opacity 200ms, filter 200ms; }
	.section :global(.tab:not([data-state='active']) .body) { opacity: 0.7; filter: saturate(0.55); scale: 0.86; }
	.section :global(.tab:hover .body) { opacity: 1; filter: none; }
	.section :global(.tab[data-state='active'] .body) { animation: bob 4.2s ease-in-out infinite; }
	.section :global(.tab b) { font-size: 14px; font-weight: 500; }
	.label { font-size: 12px; text-align: center; }

	.caption-row { display: flex; align-items: center; justify-content: center; gap: 10px; margin-bottom: 14px; }
	.caption { font-size: 14px; color: var(--foreground); text-wrap: balance; animation: fade 300ms var(--ease-out); }
	.play { display: grid; flex-shrink: 0; place-items: center; width: 28px; height: 28px; border: 1px solid var(--border); border-radius: 999px; background: var(--background); color: var(--muted-foreground); cursor: pointer; transition: color 150ms, border-color 150ms; }
	.play:hover { color: var(--primary); border-color: var(--border-stronger); }

	.features { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 28px 40px; margin: 48px 0 0; padding: 0; list-style: none; }
	.features li { display: grid; grid-template-columns: auto 1fr; gap: 4px 10px; align-content: start; }
	.features :global(svg) { grid-row: 1 / 3; margin-top: 1px; color: var(--primary); }
	.features b { font-size: 14px; font-weight: 500; color: var(--heading); }
	.features span { font-size: 13px; line-height: 1.6; color: var(--muted-foreground); text-wrap: pretty; }

	@keyframes bob { 50% { translate: 0 -5px; } }
	@keyframes fade { from { opacity: 0; } }
	@media (prefers-reduced-motion: reduce) { .section :global(.tab[data-state='active'] .body), .caption { animation: none; } }

	@media (max-width: 900px) {
		.features { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 24px 28px; }
	}

	@media (max-width: 600px) {
		.section :global(.tabs) { gap: 4px; margin: 28px 0 16px; }
		.section :global(.tab) { flex: 1; width: auto; padding-inline: 2px; }
		.body { width: 48px; height: 48px; }
		.label { font-size: 11px; line-height: 1.35; }
		.caption-row { align-items: flex-start; justify-content: space-between; }
		.caption { font-size: 13px; line-height: 1.6; }
		.features { grid-template-columns: 1fr; gap: 16px; margin-top: 32px; }
	}
</style>
