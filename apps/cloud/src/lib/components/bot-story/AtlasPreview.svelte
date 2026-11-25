<script lang="ts">
	import EyeSlashIcon from 'phosphor-svelte/lib/EyeSlashIcon';
	import github from '$lib/assets/auth/github-light.svg';
	import { m } from '$lib/paraglide/messages';

	let { step }: { step: number } = $props();
</script>

{#snippet logo()}
	<span class="logo">
		<svg viewBox="0 0 20 20" aria-hidden="true">
			<rect width="20" height="20" rx="5" fill="#d79921" />
			<path d="M5.2 14.2 10 5.6l4.8 8.6h-3.1L10 11.1l-1.7 3.1Z" fill="#fff" />
		</svg>
		Atlas
	</span>
{/snippet}

<div class="site login" class:enter={step === 1}>
	<aside class="brand-panel">
		{@render logo()}
		<div class="brand-copy"><b>{m.story_local_brand_title()}</b><span>{m.story_local_brand_description()}</span></div>
		<span class="brand-foot">
			<span class="faces"><i style="background:#d79921"></i><i style="background:#b16286"></i><i style="background:#689d6a"></i></span>
			{m.story_local_brand_footer()}
		</span>
	</aside>
	<div class="form">
		<span class="only-narrow">{@render logo()}</span>
		<b class="heading">{m.story_local_welcome()}</b>
		<span class="sub">{m.story_local_subtitle()}</span>
		<span class="btn ghost"><img src={github} alt="" width="14" height="14" />{m.story_local_github()}</span>
		<span class="or"><i></i>{m.story_local_or()}<i></i></span>
		<span class="label">{m.story_local_email()}</span>
		<span class="input">ada@northwind.dev</span>
		<span class="label split">{m.story_local_password()}<span class="text-link">{m.story_local_forgot()}</span></span>
		<span class="input focus"><span class="dots">••••••••••</span><span class="caret"></span><span class="eye"><EyeSlashIcon size={14} /></span></span>
		<span class="btn primary">{m.story_local_sign_in()}</span>
		<span class="foot">{m.story_local_new_here()} <span class="text-link">{m.story_local_create_account()}</span></span>
	</div>
</div>

<div class="site mail" class:enter={step === 2}>
	<div class="mail-card">
		<span class="mail-strip"></span>
		{@render logo()}
		<b class="heading">{m.story_local_reset_title()}</b>
		<p>{m.story_local_reset_body()} <b>ada@northwind.dev</b>.</p>
		<span class="btn primary">{m.story_local_reset_button()}</span>
		<p class="note">{m.story_local_reset_note()}</p>
	</div>
	<span class="mail-foot">Atlas Labs · 548 Market St, San Francisco</span>
</div>

<style>
	/* Both pages size the same grid cell, including while the first frame is empty. */
	.site {
		grid-area: 1 / 1;
		visibility: hidden;
		--ink: #282828; --soft: #7c6f64; --brand: #407e81;
		display: flex; min-width: 0; overflow: hidden; border-radius: 6px; background: #fffefb; color: #504945; font-size: 12px; line-height: 1.45;
		box-shadow: 0 6px 18px -12px rgb(60 56 54 / 35%);
	}
	.text-link { color: var(--brand); font-weight: 500; }
	.heading { color: var(--ink); font-weight: 600; letter-spacing: -0.01em; }
	.sub { color: var(--soft); }
	.logo { display: inline-flex; align-items: center; gap: 7px; color: var(--ink); font-size: 13px; font-weight: 600; }
	.logo svg { width: 18px; height: 18px; flex: none; }
	.btn { display: inline-flex; align-items: center; justify-content: center; gap: 7px; height: 32px; padding: 0 12px; border-radius: 6px; font-size: 12px; font-weight: 500; white-space: nowrap; }
	.primary { color: #fff; background: var(--brand); box-shadow: inset 0 -1px 0 rgb(0 0 0 / 12%); }
	.ghost { color: var(--ink); background: #fff; border: 1px solid #ddd8cc; box-shadow: 0 1px 1px rgb(60 56 54 / 6%); }
	.only-narrow { display: none; }

	.login { display: grid; grid-template-columns: 40% 60%; }
	.brand-panel { position: relative; display: flex; flex-direction: column; justify-content: space-between; gap: 16px; padding: 18px; overflow: hidden; color: #ebdbb2;
		background: radial-gradient(120% 70% at 100% 100%, rgb(215 153 33 / 16%), transparent 60%), radial-gradient(circle at 1px 1px, rgb(251 241 199 / 10%) 1px, transparent 0) 0 0 / 14px 14px, linear-gradient(160deg, #3c7478, #23484a); }
	.brand-panel .logo { color: #fbf1c7; }
	.brand-copy { display: grid; gap: 6px; margin-top: auto; }
	.brand-copy b { color: #fbf1c7; font-size: 16px; font-weight: 600; line-height: 1.25; letter-spacing: -0.01em; }
	.brand-copy span { color: rgb(235 219 178 / 78%); font-size: 11.5px; }
	.brand-foot { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 8px; white-space: nowrap; color: rgb(235 219 178 / 70%); font-size: 10.5px; }
	.faces { display: flex; }
	.faces i { width: 16px; height: 16px; margin-right: -5px; border: 1.5px solid #2c5658; border-radius: 50%; }
	.faces i:last-child { margin-right: 0; }
	.form { display: flex; flex-direction: column; justify-content: center; padding: 16px 26px; }
	.form .heading { font-size: 16px; }
	.form .sub { margin: 1px 0 12px; }
	.or { display: flex; align-items: center; gap: 8px; margin: 8px 0 6px; color: #a89984; font-size: 10.5px; }
	.or i { flex: 1; height: 1px; background: #e8e4da; }
	.label { margin-bottom: 4px; color: var(--ink); font-size: 11px; font-weight: 500; }
	.split { display: flex; justify-content: space-between; margin-top: 8px; }
	.split .text-link { font-size: 11px; }
	.input { display: flex; align-items: center; height: 28px; padding: 0 10px; border: 1px solid #ddd8cc; border-radius: 6px; background: #fff; color: var(--ink); }
	.eye { margin-left: auto; color: #a89984; }
	.focus { border-color: #458588; box-shadow: 0 0 0 3px rgb(69 133 136 / 16%); }
	.dots { letter-spacing: 1px; font-size: 11px; }
	.caret { width: 1px; height: 14px; margin-left: 2px; background: var(--ink); }
	.form .btn { height: 30px; }
	.form .primary { margin-top: 12px; }
	.foot { margin-top: 10px; color: var(--soft); text-align: center; font-size: 11px; }

	.mail { flex-direction: column; align-items: center; gap: 10px; padding: 20px 16px 14px; background: #f2eee4; }
	.mail-card { position: relative; display: grid; justify-items: start; gap: 10px; width: min(100%, 330px); padding: 22px 24px 20px; overflow: hidden; border: 1px solid #e6e0d2; border-radius: 8px; background: #fff; }
	.mail-strip { position: absolute; inset: 0 0 auto; height: 3px; background: linear-gradient(90deg, #407e81, #689d6a 60%, #d79921); }
	.mail-card .logo { margin-bottom: 4px; }
	.mail-card .heading { font-size: 17px; }
	.mail-card p { margin: 0; }
	.mail-card p b { color: var(--ink); font-weight: 500; }
	.mail-card .btn { margin: 4px 0 2px; padding: 0 16px; }
	.note { padding-top: 10px; border-top: 1px solid #efebe2; color: var(--soft); font-size: 11px; }
	.mail-foot { color: #a89984; font-size: 10px; }
	.site.enter { visibility: visible; }
	@media (prefers-reduced-motion: no-preference) {
		.enter .caret { animation: blink 1s step-end infinite; }
	}
	@keyframes blink { 50% { opacity: 0; } }
	@media (min-width: 761px) and (max-width: 960px) {
		.brand-panel { padding-inline: 10px; }
	}
	@media (max-width: 760px) {
		.login { grid-template-columns: 1fr; }
		.brand-panel { display: none; }
		.only-narrow { display: block; margin-bottom: 14px; }
		.form { padding: 18px 18px 16px; }
		.mail { padding: 14px 10px 12px; }
		.mail-card { padding: 18px 16px 16px; }
	}
</style>
