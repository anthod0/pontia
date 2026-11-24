<script lang="ts">
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import ListChecksIcon from 'phosphor-svelte/lib/ListChecksIcon';
	import ArrowBendUpLeftIcon from 'phosphor-svelte/lib/ArrowBendUpLeftIcon';
	import AgentBadge from './AgentBadge.svelte';
	import StoryState from './StoryState.svelte';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import type { Chat, Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, chat, step }: { story: Story; chat: Chat; step: number } = $props();
	const status = $derived(chat.status[step]);
	const todoCount = $derived(chat.todos[step]);
	const sessions = $derived(chat.sessions.filter((session) => session.states[step] !== 'hidden'));
</script>

<!-- A scripted Bot chat on the left and the Sessions it runs on the right. -->
<div class="demo">
	<div class="chat" style:--bot={story.color}>
		<div class="chat-bar">
			<span class="avatar"><BotAvatarMark shape={story.shape} color={story.color} /></span>
			<span class="name">{story.name}</span>
			<span class={['todo-button', todoCount > 0 && 'has-todo']}>
				<ListChecksIcon size={13} />{m.story_todo_badge()}
				{#if todoCount > 0}<span class="count">{todoCount}</span>{/if}
			</span>
		</div>
		<ol class="feed">
			{#each chat.lines as line, index (index)}
				{#if index <= step}
					<li class={['line', line.kind]}>
						{#if line.related}
							<span class="related"><ArrowBendUpLeftIcon size={11} />{m.story_related()} · <span>{line.related()}</span></span>
						{/if}
						<span class={['bubble', line.todo && 'todo']}>
							{#if line.quote}<span class="quote">{line.quote()}</span>{/if}
							<span>{line.text()}</span>
						</span>
						{#if line.todo}
							{@const closed = chat.todoClosedAt !== undefined && step >= chat.todoClosedAt}
							<span class={['todo-tag', closed && 'closed']}>
								{#if closed}<CheckIcon size={10} weight="bold" />{story.name} · {m.story_todo_closed()}{:else}<i></i>{m.story_todo_badge()}{/if}
							</span>
						{/if}
					</li>
				{/if}
			{/each}
		</ol>
		<div class="status">
			{#if status}<span class="avatar small working"><BotAvatarMark shape={story.shape} color={story.color} /></span><span class="status-text">{status()}</span>{/if}
		</div>
	</div>

	<ul class="sessions">
		{#each sessions as session (session.title())}
			{@const state = session.states[step]}
			<li class="session card enter">
				<AgentBadge agent={session} />
				<span class="title">{session.title()}</span>
				<span class="state-cell"><StoryState {state} /></span>
			</li>
		{/each}
	</ul>
</div>

<style>
	.demo { display: grid; grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr); gap: 40px; align-items: center; }

	.chat { display: flex; flex-direction: column; height: 364px; border: 1px solid var(--border); border-radius: 12px; background: var(--background); box-shadow: 0 20px 40px -32px rgb(60 56 54 / 45%); overflow: hidden; }
	.chat-bar { display: flex; align-items: center; gap: 8px; height: 42px; padding-inline: 14px; border-bottom: 1px solid var(--border); font-size: 13px; font-weight: 500; color: var(--heading); }
	.avatar { width: 22px; height: 22px; flex-shrink: 0; }
	.avatar.small { width: 16px; height: 16px; }
	.working { animation: bob 1.6s ease-in-out infinite; }
	.todo-button { display: inline-flex; align-items: center; gap: 5px; margin-left: auto; padding: 3px 8px; border: 1px solid var(--border); border-radius: 7px; font-size: 11px; font-weight: 500; color: var(--muted-foreground); transition: color 200ms, border-color 200ms; }
	.todo-button.has-todo { color: var(--heading); border-color: color-mix(in srgb, var(--primary) 45%, var(--border)); }
	.count { display: grid; place-items: center; min-width: 15px; height: 15px; padding-inline: 4px; border-radius: 999px; background: var(--primary-solid); color: white; font-size: 10px; animation: pop 160ms var(--ease-out); }

	.feed { display: flex; flex: 1; flex-direction: column; justify-content: flex-end; gap: 8px; min-height: 0; margin: 0; padding: 14px; list-style: none; overflow: hidden; mask-image: linear-gradient(to bottom, transparent, #000 28px); }
	.line { display: flex; flex-direction: column; gap: 3px; max-width: 86%; animation: enter 180ms var(--ease-out) both; }
	.line.user { align-self: flex-end; align-items: flex-end; }
	.line.bot { align-self: flex-start; align-items: flex-start; }
	.bubble { display: inline-flex; flex-direction: column; gap: 4px; padding: 8px 12px; border-radius: 14px; font-size: 13px; line-height: 1.45; }
	.user .bubble { border-bottom-right-radius: 4px; background: var(--button); color: white; }
	.bot .bubble { border: 1px solid var(--border); border-bottom-left-radius: 4px; background: var(--surface); color: var(--foreground); }
	.bubble.todo { border-color: color-mix(in srgb, var(--primary) 55%, var(--border)); }
	.quote { padding-left: 8px; border-left: 2px solid rgb(255 255 255 / 55%); font-size: 12px; opacity: 0.85; display: -webkit-box; -webkit-line-clamp: 1; line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden; }
	.related { display: inline-flex; align-items: center; gap: 4px; max-width: 100%; font-size: 11px; color: var(--muted-foreground); white-space: nowrap; }
	.related > span { overflow: hidden; text-overflow: ellipsis; }
	.todo-tag { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; font-weight: 500; color: var(--primary); }
	.todo-tag i { width: 6px; height: 6px; border-radius: 50%; background: var(--primary); }
	.todo-tag.closed { color: var(--muted-foreground); }
	.status { display: flex; align-items: center; gap: 8px; height: 34px; padding-inline: 14px; border-top: 1px solid var(--border); font-size: 12px; color: var(--muted-foreground); }
	.status-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.sessions { display: flex; flex-direction: column; gap: 12px; margin: 0; padding: 0; list-style: none; }
	.session { display: grid; grid-template-columns: 1fr auto; gap: 4px 14px; align-items: center; padding: 12px 16px; }
	.title { grid-column: 1; font-size: 14px; font-weight: 500; color: var(--heading); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.state-cell { grid-column: 2; grid-row: 1 / 3; }

	@keyframes enter { from { opacity: 0; translate: 0 6px; } }
	@keyframes pop { from { scale: 0.4; } }
	@keyframes bob { 50% { translate: 0 -2px; } }
	@media (prefers-reduced-motion: reduce) { .line, .count, .working { animation: none; } }

	/* Narrow screens: the chat fills the width and the Sessions follow as a compact list. */
	@media (max-width: 760px) {
		.demo { grid-template-columns: 1fr; gap: 14px; }
		.chat { height: 340px; }
		.sessions { gap: 8px; }
		.session { padding: 9px 12px; }
		.title { font-size: 13px; }
	}
</style>
