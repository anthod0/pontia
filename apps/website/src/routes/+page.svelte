<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';
	import MonitorIcon from 'phosphor-svelte/lib/MonitorIcon';
	import PlayPauseIcon from 'phosphor-svelte/lib/PlayPauseIcon';
	import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon';
	import HeroSection from '$lib/components/HeroSection.svelte';

	const repository = 'https://github.com/anthod0/pontia';
	let copyStatus = $state<'idle' | 'copied' | 'error'>('idle');

	async function copyCommand() {
		try {
			await navigator.clipboard.writeText('pontia init');
			copyStatus = 'copied';
		} catch {
			copyStatus = 'error';
		}
	}
</script>

<svelte:head>
	<title>Pontia — Beyond the terminal window.</title>
	<meta name="description" content="Keep your coding agents working beyond a single terminal window. Pontia gives you persistent pi sessions and a web dashboard, without giving up the real agent TUI." />
	<meta property="og:title" content="Pontia — Beyond the terminal window." />
	<meta property="og:description" content="Beyond the terminal. Still working. Persistent sessions, a web dashboard, and the terminal you already know." />
	<meta property="og:type" content="website" />
	<meta name="twitter:card" content="summary" />
</svelte:head>

<a class="skip-link" href="#main">Skip to content</a>

<header class="site-header page-width">
	<a class="brand" href="/" aria-label="Pontia home"><img src="/logo.svg" alt="" /><span>Pontia</span></a>
	<nav aria-label="Primary navigation">
		<a class="nav-link" href="#features">Why Pontia</a>
		<a class="nav-link source-link" href={repository}>GitHub<ArrowUpRightIcon size={13} /></a>
		<a class="button button-small" href="#get-started">Get started<ArrowRightIcon size={14} /></a>
	</nav>
</header>

<main id="main">
	<HeroSection {repository} />

	<section id="features" class="features page-width section-block" aria-labelledby="features-title">
		<div class="section-intro">
			<h2 id="features-title">Leave the window.<br />Keep the context.</h2>
			<p>Your agent is more than a tab in your terminal.<br /> Give it a persistent place to work, and yourself<br class="desktop-break" /> a clearer view of what’s happening.</p>
		</div>
		<div class="feature-grid">
			<article class="feature">
				<div class="feature-top"><TerminalWindowIcon size={25} weight="light" /><span>01</span></div>
				<h3>A session, not a window.</h3>
				<p>Keep agent sessions running beyond your terminal window. Return to the conversation without starting from scratch.</p>
				<div class="feature-detail"><span class="status-dot"></span>Persistent by design</div>
			</article>
			<article class="feature">
				<div class="feature-top"><MonitorIcon size={25} weight="light" /><span>02</span></div>
				<h3>Two surfaces. One agent.</h3>
				<p>Work in the real pi TUI or continue in the web dashboard. Same session, same conversation. No parallel agent behind the scenes.</p>
				<div class="feature-detail"><span class="surface-tag">Terminal</span><span>↔</span><span class="surface-tag">Web</span></div>
			</article>
			<article class="feature">
				<div class="feature-top"><PlayPauseIcon size={25} weight="light" /><span>03</span></div>
				<h3>Less guessing. More control.</h3>
				<p>See your workspaces and sessions in one place. Follow the conversation, send the next instruction, or stop and resume when you need to.</p>
				<div class="feature-detail"><span class="status-dot ochre"></span>Your work, in view</div>
			</article>
		</div>
	</section>

	<section class="principle page-width" aria-label="Local-first architecture">
		<div class="principle-symbol" aria-hidden="true"><TerminalWindowIcon size={24} weight="light" /></div>
		<p><strong>Your machine. Your tools. Your sessions.</strong><br />Pontia runs locally, around the real agent TUI. Not a replacement for your workflow. A way to keep it going.</p>
		<span class="principle-label">NO CLOUD ACCOUNT REQUIRED</span>
	</section>

	<section id="get-started" class="getting-started page-width section-block" aria-labelledby="start-title">
		<div class="setup-copy">
			<h2 id="start-title">A small setup.<br />A longer-running session.</h2>
			<p class="setup-description">Bring your terminal. We’ll keep the lights on.</p>
			<ol class="setup-steps">
				<li><span class="step-number">01</span><div><h3>Grab the binaries</h3><p>Download <code>pontia</code> and <code>pontiad</code> for your platform. Put both on your <code>PATH</code>.</p><a class="text-link" href={`${repository}/releases/latest`}>Download the latest release<ArrowUpRightIcon size={14} /></a></div></li>
				<li><span class="step-number">02</span><div><h3>Bring pi and tmux</h3><p>Install <a href="https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent">pi CLI</a> and <a href="https://github.com/tmux/tmux/wiki/Installing">tmux</a> if you don’t already have them.</p></div></li>
				<li><span class="step-number">03</span><div><h3>Make yourself at home</h3><p>Run <code>pontia init</code>. Follow the prompts to set up the pi integration, start the service, and open your dashboard.</p></div></li>
			</ol>
		</div>
		<div class="setup-side">
			<div class="setup-terminal">
				<div class="setup-terminal-header"><span><TerminalWindowIcon size={15} />Quick start</span><span>~/</span></div>
				<div class="setup-terminal-body">
					<p class="terminal-comment"># Your first session starts here</p>
					<div class="command-line"><code><span>$</span> pontia init</code><button class="copy-button" onclick={copyCommand} aria-label={copyStatus === 'copied' ? 'Copy command again' : 'Copy pontia init command'} title="Copy command">{#if copyStatus === 'copied'}<CheckIcon size={16} />{:else}<CopyIcon size={16} />{/if}</button></div>
					<p class="copy-feedback" aria-live="polite">{copyStatus === 'copied' ? 'Copied to clipboard.' : copyStatus === 'error' ? 'Could not copy. Select “pontia init” above to copy manually.' : ''}</p>
					<div class="terminal-divider"></div>
					<p class="terminal-comment"># All set? You’re in control.</p>
					<div class="terminal-command"><code><span>$</span> pontia status</code><span>Check in</span></div>
					<div class="terminal-command"><code><span>$</span> pontia down</code><span>Power down</span></div>
					<div class="terminal-command"><code><span>$</span> pontia up</code><span>Pick back up</span></div>
				</div>
				<div class="setup-terminal-footer"><span class="status-dot"></span>Local by default. Ready when you are.</div>
			</div>
			<p class="platform-note">Linux with systemd & macOS.<br />No Rust, Cargo, or frontend build tools needed.</p>
		</div>
	</section>

	<section class="development-note page-width" aria-labelledby="development-title">
		<div class="development-label"><span class="status-dot ochre"></span>WORK IN PROGRESS</div>
		<div><h2 id="development-title">Built in the open. Still taking shape.</h2><p>Pontia is experimental and intended for local development. Expect rough edges and breaking changes. Try it, explore the source, and help shape what comes next.</p></div>
		<a class="text-link" href={`${repository}/issues`}>Join the conversation<ArrowUpRightIcon size={15} /></a>
	</section>
</main>

<footer class="site-footer page-width">
	<a class="brand footer-brand" href="/" aria-label="Pontia home"><img src="/logo.svg" alt="" /><span>Pontia</span></a>
	<p>Beyond the terminal window.</p>
	<nav aria-label="Footer navigation"><a href={repository}>GitHub<ArrowUpRightIcon size={12} /></a><a href={`${repository}#get-started`}>Documentation</a><a href={`${repository}/blob/main/LICENSE`}>Apache 2.0</a></nav>
</footer>
