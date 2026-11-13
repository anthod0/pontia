<script lang="ts">
	import AuthProviderIcon from '$lib/components/AuthProviderIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import { authError } from '$lib/auth-errors';
	import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
	import { m } from '$lib/paraglide/messages';
	import { localizeHref } from '$lib/paraglide/runtime';
	import type { PageData } from './$types';
	let { data }: { data: PageData } = $props();
	const message = $derived(authError(data.error));
	const returnToQuery = $derived(
		data.returnTo ? `?return_to=${encodeURIComponent(data.returnTo)}` : ''
	);

	const sessions = [
		{ title: 'Add keyboard shortcuts', client: 'codex', state: 'idle', label: 'Ready' },
		{ title: 'Fix flaky upload test', client: 'claude', state: 'busy', label: 'Working' },
		{ title: 'Review migration plan', client: 'pi', state: 'waiting', label: 'Needs approval' }
	];
</script>

<svelte:head
	><title>{m.login_title()}</title><meta
		name="robots"
		content="noindex"
	/></svelte:head
>

<div class="login">
	<aside class="showcase" aria-hidden="true">
		<a class="showcase-brand" href={localizeHref('/')} tabindex="-1">
			<img src="/logo.svg" alt="" />
			<span>Pontia</span>
		</a>
		<div class="showcase-body">
			<div class="showcase-copy">
				<p class="showcase-title">{m.login_showcase_title_first()}<br /><span>{m.login_showcase_title_second()}</span></p>
				<p class="showcase-text">
					{m.login_showcase_text()}
				</p>
			</div>
			<div class="preview">
				<div class="preview-bar">
					<span class="device-dot"></span>
					<span>Studio MacBook</span>
					<span class="preview-status">Online</span>
				</div>
				<ul>
					{#each sessions as session}
						<li>
							<span class={['state', session.state]}></span>
							<span class="session-title">{session.title}</span>
							<span class="session-meta">{session.client}</span>
							<span class={['badge', session.state]}>{session.label}</span>
						</li>
					{/each}
				</ul>
			</div>
		</div>
	</aside>

	<main class="panel">
		<a class="panel-brand" href={localizeHref('/')} aria-label={m.home_link_label()}>
			<img src="/logo.svg" alt="" />
			<span>Pontia</span>
		</a>
		<div class="card">
			<h1>{m.login_heading()}</h1>
			<p class="lead">{m.login_lead()}</p>
			{#if message}<p class="auth-error" role="alert">{message}</p>{/if}
			<div class="providers">
				<form method="POST" action={`/api/auth/google/login${returnToQuery}`}>
					<Button type="submit" class="button button-secondary provider"
						><AuthProviderIcon provider="google" />{m.continue_with_google()}</Button
					>
				</form>
				<form method="POST" action={`/api/auth/github/login${returnToQuery}`}>
					<Button type="submit" class="button button-secondary provider"
						><AuthProviderIcon provider="github" />{m.continue_with_github()}</Button
					>
				</form>
			</div>
			{#if data.hasCredential}
				<form method="POST" action="/api/auth/logout" class="sign-out">
					<Button type="submit" variant="link">{m.login_sign_out_current()}</Button>
				</form>
			{/if}
			<p class="legal-notice">
				<ParaglideMessage message={m.login_legal}>
					{#snippet terms({ children })}<a href="/terms">{@render children?.()}</a>{/snippet}
					{#snippet privacy({ children })}<a href="/privacy">{@render children?.()}</a>{/snippet}
				</ParaglideMessage>
			</p>
		</div>
		<p class="help">
			<ParaglideMessage message={m.login_help}>
				{#snippet guide({ children })}<a href={localizeHref('/docs/getting-started')}>{@render children?.()}</a>{/snippet}
			</ParaglideMessage>
		</p>
	</main>
</div>

<style>
	.login {
		display: grid;
		grid-template-columns: minmax(0, 1.05fr) minmax(0, 1fr);
		min-height: 100svh;
	}

	.showcase {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 48px;
		padding: 40px 56px;
		overflow: hidden;
		background:
			radial-gradient(ellipse 70% 45% at 35% 55%, rgb(69 133 136 / 28%), transparent 70%),
			radial-gradient(circle at 1px 1px, rgb(235 219 178 / 9%) 1px, transparent 0) 0 0 / 22px 22px,
			#282828;
		color: #ebdbb2;
	}
	.showcase-brand,
	.panel-brand {
		display: inline-flex;
		align-items: center;
		gap: 9px;
		font-size: 19px;
		font-weight: 600;
		letter-spacing: -0.5px;
	}
	.showcase-brand img,
	.panel-brand img {
		width: 34px;
		height: 34px;
	}
	.showcase-body {
		display: flex;
		flex: 1;
		flex-direction: column;
		justify-content: center;
		gap: 40px;
		padding-bottom: 40px;
	}
	.showcase-title {
		font-size: clamp(32px, 3.6vw, 48px);
		font-weight: 500;
		line-height: 1.08;
		letter-spacing: -0.045em;
		color: #fbf1c7;
	}
	.showcase-title span {
		color: #8ec07c;
	}
	.showcase-text {
		max-width: 400px;
		margin-top: 18px;
		font-size: 15px;
		line-height: 1.65;
		color: #bdae93;
	}

	.preview {
		max-width: 460px;
		border: 1px solid #45433e;
		border-radius: 10px;
		background: rgb(29 32 33 / 72%);
		box-shadow: 0 30px 60px -30px rgb(0 0 0 / 60%);
		font-size: 13px;
	}
	.preview-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px 16px;
		border-bottom: 1px solid #3c3836;
		color: #d5c4a1;
		font-weight: 500;
	}
	.device-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: #98971a;
		box-shadow: 0 0 0 3px rgb(152 151 26 / 20%);
	}
	.preview-status {
		margin-left: auto;
		font: 11px var(--font-mono);
		color: #928374;
	}
	.preview ul {
		margin: 0;
		padding: 6px;
		list-style: none;
	}
	.preview li {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto auto;
		align-items: center;
		gap: 10px;
		padding: 10px;
		border-radius: 6px;
	}
	.preview li:first-child {
		background: rgb(235 219 178 / 6%);
	}
	.state {
		width: 7px;
		height: 7px;
		border-radius: 50%;
	}
	.state.idle {
		background: #98971a;
	}
	.state.busy {
		background: #d79921;
		animation: pulse 1.6s ease-in-out infinite;
	}
	.state.waiting {
		background: #d3869b;
	}
	.session-title {
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		color: #ebdbb2;
	}
	.session-meta {
		font: 11px var(--font-mono);
		color: #928374;
	}
	.badge {
		padding: 2px 8px;
		border-radius: 999px;
		font-size: 11px;
		white-space: nowrap;
	}
	.badge.idle {
		background: rgb(152 151 26 / 16%);
		color: #b8bb26;
	}
	.badge.busy {
		background: rgb(215 153 33 / 16%);
		color: #fabd2f;
	}
	.badge.waiting {
		background: rgb(211 134 155 / 16%);
		color: #d3869b;
	}

	.panel {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 24px;
		padding: 48px 24px;
		background:
			radial-gradient(ellipse 60% 40% at 50% 40%, rgb(69 133 136 / 7%), transparent 70%),
			var(--background);
	}
	.panel-brand {
		display: none;
	}
	.card {
		width: min(400px, 100%);
		padding: 36px 32px 28px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--background);
		box-shadow: 0 24px 48px -32px rgb(40 60 60 / 30%);
	}
	h1 {
		font-size: 26px;
		font-weight: 500;
		letter-spacing: -0.035em;
	}
	.lead {
		margin-top: 8px;
		font-size: 14px;
		line-height: 1.6;
		color: var(--muted-foreground);
	}
	.auth-error {
		margin-top: 20px;
		padding: 12px;
		border: 1px solid var(--destructive-border);
		border-radius: 4px;
		color: var(--destructive);
		font-size: 14px;
	}
	.providers {
		display: grid;
		gap: 10px;
		margin-block: 28px 20px;
	}
	.providers :global(.provider) {
		width: 100%;
		background: var(--background);
		font-size: 14px;
	}
	.sign-out {
		margin-bottom: 16px;
		text-align: center;
	}
	.legal-notice {
		padding-top: 18px;
		border-top: 1px solid var(--border);
		font-size: 12px;
		line-height: 1.6;
		color: var(--muted-foreground);
	}
	.legal-notice a,
	.help a {
		color: var(--primary);
		text-decoration: underline;
		text-underline-offset: 3px;
	}
	.help {
		font-size: 13px;
		color: var(--muted-foreground);
	}

	@keyframes pulse {
		50% {
			opacity: 0.35;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.state.busy {
			animation: none;
		}
	}
	@media (max-width: 900px) {
		.login {
			grid-template-columns: 1fr;
		}
		.showcase {
			display: none;
		}
		.panel {
			justify-content: flex-start;
			padding-top: 56px;
		}
		.panel-brand {
			display: inline-flex;
			margin-bottom: 8px;
		}
	}
	@media (max-width: 480px) {
		.card {
			padding: 28px 20px 22px;
			border: 0;
			box-shadow: none;
		}
	}
</style>
