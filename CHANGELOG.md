# Changelog

All notable changes to Pontia will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Bots send separate messages for unrelated matters, even when updates arrive together.
- On phones, the Bot page top bar no longer has a **More** menu. Tap the Bot name to open its settings, where **Delete Bot** is at the bottom. A downloads button appears in the top bar while downloads are in progress or failed.
- Bot chats open with the latest messages saved in the browser and then update from the device, so recent messages stay visible while offline or on a slow connection. Deleting a Bot also removes its saved messages from the browser.

### Fixed

- Bot chat loading placeholders use the same gray surface for user and Bot messages.

## [0.8.5] - 2026-11-21

### Added

- Bot chat messages and shared files have a menu: right-click on a computer, press and hold on a phone, or select part of a message. **Quote** adds the message, selection, or file above the composer, and **Copy** copies the message or selection. The Bot receives the quote as a `<user_quote>` element before the message text, with the full quoted text. Clicking a quote in a sent message scrolls back to the original, loading older history when needed.
- Bots can be deleted from the Dashboard: use **Delete Bot** at the bottom of the Bot settings, or in the top bar's **More** menu on phones. Deletion permanently removes the Bot's chat history, its links to helpers, notifications, and Workspaces, and its directory under `$PONTIA_HOME/bot`. Memories, `USER.md`, and Workspaces are kept. A Bot that is working or has open helpers cannot be deleted; the confirmation lists the open helpers to close first. Deleting the last Bot pauses dreaming until a new Bot is created.
- `pontia remote edit` can switch Edge interactively while keeping the device identity and Dashboard URL. Each prompt defaults to the current value; unchanged settings leave the service running. Edge changes restart a running managed service and report when a manual restart is needed.

### Changed

- Session chats use the same file bubbles as Bot chats, with inline image previews, download progress, cancellation, and retries. Session images can still be browsed together in the image viewer.
- The homepage shows a Bot taking one request, splitting it across three agents, and reporting back when the work is done, in place of the section listing individual agent Sessions.
- Messages from a Bot to its helpers no longer include the Bot Session ID.
- Bot, memory retrieval, and dreaming Sessions no longer automatically load user skills. Ordinary helpers retain their existing skill loading.
- Bots use `pontia` commands, read only known file paths, and delegate exploration and system changes to helpers, with at most one external read-only lookup per input.
- Bot chats show a skeleton of the conversation and composer while loading, instead of a loading message.
- The Downloads dialog lists only downloads in progress and failed downloads. Finished downloads leave the list, failed ones can be retried or dismissed, and the dialog closes when the list is empty.
- Shared files and local file links open the text preview panel only for text and code files and files without a known extension; images still open in the image viewer. Videos, audio, PDFs, archives, and Office documents are download-only, and clicking a local file link to one of them downloads it. Session chats and Bot chats use the same rules.
- The self-hosted edge deployment page shows the generated command and the retry command in the same code blocks as the Cloud docs, each with its own copy button.
- The Dashboard's Agent Profiles page uses the same list-and-editor layout as Agent Clients. New Profiles suggest an ID from their name, editing shows the ID with a copy button, and Delete is at the bottom of the editor and asks for confirmation.
- The Dashboard requires a client and a model for every Agent Profile it saves, so each saved Profile can be offered to Bots.
- **Breaking:** New Agent Profile IDs must start with a lowercase letter or digit and contain only lowercase letters, digits, `.`, `_`, and `-`. Existing Profiles keep their IDs and can still be edited, used, and deleted.

### Fixed

- Device login no longer fails when the login service closes an idle connection between approval checks.

## [0.8.4] - 2026-11-21

### Added

- Bots see the Agent Profiles that can start a helper, with each Profile's name, description, client, and model, so they can pick one with `--profile-id` without querying the device.

### Changed

- The homepage now presents example agent Sessions and remote control features, including terminal synchronization and end-to-end encryption.
- Bot file messages show file-type icons and download progress, with inline image previews and a preview panel for text files.
- Quick start guides now use three steps: install Pontia, initialize the device, and open the Dashboard. Platform guides include service startup instructions.
- Bots delegate worktree creation to their helpers.
- Bot work status now shows message processing first, then the title of the newest busy helper, and disappears when idle. Memory retrieval and dreaming do not count as background work. Bot chats and sidebar activity update as helper Sessions change.
- **Breaking:** `pontia bot send-to-user` no longer accepts `--working-on`, and its API no longer accepts `working_on`. Bot records no longer expose that field; chat and message-page responses replace `processing` with `status` (`processing`, `working` with a title, or `idle`).
- **Breaking:** `pontia bot helper create` requires a nonempty `--title`, and the helper creation API requires `title`. Memory retrieval is unchanged.
- Bot chat messages scroll beneath the transparent top bar, and the work status above the composer is larger and easier to read.
- `pontia bot helper create` without `--profile-id` starts a general helper that uses the Bot's current client and model. Bots no longer create a `<bot_id>-helper` Agent Profile, so changing a Bot's model in its settings also applies to new helpers. Existing `<bot_id>-helper` Profiles remain ordinary Profiles and can be deleted.

## [0.8.3] - 2026-11-20

### Added

- `pontia bot helper create` accepts `--title TEXT` to name the helper's Session in the Dashboard.

### Changed

- Bots now start deep memory retrieval with `pontia bot memory retrieve`. This replaces `pontia bot helper retrieve` and removes the retrieval option `--profile-version` and its API field `profile_version`. Dreaming and deep retrieval now use built-in instructions. Deep retrieval uses the requesting Bot’s client and model. Dreaming uses those of the oldest Bot on the device.

## [0.8.2] - 2026-11-20

### Added

- The chat composer offers `/pin` and `/archive` to pin, unpin, archive, or unarchive the current Session.
- Files pasted into the chat composer are attached to the message.
- The file preview panel previews shared text files.
- Bot avatars animate while the Bot is working, and the Bot work status text shimmers.

### Changed

- `pontia bot helper create` now requires either `--workspace-id ID` for project work or `--temporary NAME` for a one-off task. Bots prepare the default Workspace at `~/Pontia` and keep each temporary task’s files in `~/Pontia/NAME`.
- `pontia update` and `pontia uninstall` now list blocking Sessions by title, Agent Client, and Workspace with a plain reason, instead of Session IDs.
- `Alt+M` switches devices from any Dashboard page, not only New chat.
- The mobile top bar menu shows Downloads only when there are downloads, with a badge counting them.
- When Bots exist, the create Bot button moves to the Bots sidebar group header.
- Creating a Bot no longer asks for a description; edit it later in Bot settings.

### Fixed

- `pontia update` updates an npm-installed Pi extension with `pi update` instead of reinstalling it.
- Swiping left closes the mobile sidebar; previously it closed on a right swipe.
- Mobile top bar buttons leave a gap below the top edge of the screen.

## [0.8.1] - 2026-11-20

### Added

- Bots can start helpers in an active Workspace with `pontia bot helper create --workspace-id`, and `pontia bot helper list` shows each helper's Workspace ID or unbound status. Bots reuse helpers only within the same Workspace.
- Bot chat shows a processing status until the Bot replies.

### Changed

- Bot chat renders Bot replies as Markdown and hides the message feed scrollbar.

### Fixed

- Starting a Session in a Workspace whose directory is missing reports that the Workspace directory is unavailable.

## [0.8.0] - 2026-11-20

### Added

- Device Bots: create persistent, device-level Bots backed by Pi, Claude Code, or Codex, with customizable avatars, model selection, file uploads, and onboarding that does not require a Workspace first. Bots share device memory organized in hourly background dreaming rounds, coordinate work through durable asynchronous helpers, and rotate their underlying Session automatically at context and idle limits.
- Choose the model when starting a new Session, with each Agent Client exposing its model catalog and default model. ACP Agents discover available models before Session creation.
- Claude Code Sessions report context window usage and support custom Agent Profiles with static system prompts.
- ACP Sessions inject Agent Profile instructions when the ACP Agent supports it.
- Added a Claude Code guide to the Cloud documentation, introduced Bots on the Cloud homepage, and added a Simplified Chinese README (`README.zh-CN.md`).

### Changed

- New chat displays the device, Agent Client, Workspace, and model selectors as an inline toolbar and selects the Agent Client's default model automatically.
- In New chat, use `Alt+C` for the next Agent Client and `Alt+Shift+C` for the previous one. This avoids the `Alt+A` conflict with WeChat.
- On phones, the Dashboard top bar groups chat actions (New Chat, rename Session, and edit Bot) into an overflow menu and enlarges header and sidebar touch targets.
- `pontia status` and `pontia doctor` show a spinner while they check the cloud login and remote registration.
- `pontia setup codex` writes shell `PATH` setup to `env.sh` or `env.fish` so Pontia's Codex launcher directory stays first in `PATH`, and `pontia doctor` warns when `PATH` precedence is shadowed.
- Cloud and the README use a dedicated dark Codex logo.

### Removed

- Removed in-session model switching; choose the model when starting a new Session.

### Fixed

- `Alt+M` switches devices on device-scoped New chat pages.
- Pressing the New Chat button in the sidebar preserves its text color.
- Codex restores Session observation after an interrupted TUI continuation.

## [0.7.16] - 2026-11-18

### Added

- New chat has keyboard shortcuts to switch the device (`Alt+M`), Agent Client (`Alt+A`), and Workspace (`Alt+W`). Add `Shift` to go back.
- Local file links in the Dashboard file preview open in the same sidebar, with a back button that restores the previous reading position. Relative links resolve against the previewed file, or against the session Workspace in chat messages. Heading anchors scroll to the heading, and local image links open in the image viewer.
- The file preview sidebar can be resized and has a refresh button.
- The image viewer pages between the images shared in a Session.
- Codex answers to agent questions appear as answered questions in the conversation.
- Edge settings in Cloud show the version each Edge is running. The Edge `/healthz` endpoint reports it in the `Pontia-Edge-Version` response header.
- The Cloud site slogan is translated into Simplified Chinese, Japanese, and Portuguese.

### Changed

- On phones, the sidebar opens full screen and closes after navigation. The top bar buttons are larger, and New Chat sits next to downloads.
- The shortcut help shows alternative keys for the same message input action on one line.
- The file preview shows clearer loading, empty, and error states.

### Fixed

- Pinned Sessions no longer also appear in the recent list.
- The agent status stays next to the latest Turn instead of below the conversation.

## [0.7.15] - 2026-11-18

### Added

- The Dashboard previews text files in a right sidebar.
- New chat remembers the last Agent Client you used.
- The shortcut help lists message input shortcuts.

### Changed

- Project directories must be activated as Workspaces before terminal clients connect, Sessions are created, or Workflows run. Codex runs natively in directories that are not active Workspaces.
- The download center no longer shows a description.

### Fixed

- The Dashboard keeps remote device connections, retries the device list after network failures, and retries reads after device access expires.
- Git status refreshes when you open a chat or click the Session metadata.
- Codex user messages no longer appear twice after history is rebuilt.
- Pi keeps the inbox message identity when Pontia input is delivered after a Turn ends.

## [0.7.14] - 2026-11-17

### Added

- The Dashboard is available in Chinese, with a language preference.
- Claude Code and ACP agents accept steering messages from the Dashboard. ACP agents without steering support queue the message instead.
- Pi supports steering and delivery shortcuts in the inbox.
- `pontia init` and the Dashboard detect installed agent clients, and the Dashboard can set them up through the CLI.
- The Dashboard downloads local file links with inline progress, and downloads keep running across page navigation.
- The conversation shows context compaction progress.
- Shared images open in a fullscreen viewer with zoom.

### Changed

- The conversation shows each Turn's output and steering messages in the order they happened.
- The shared files dialog shows files as a borderless list.

### Fixed

- Queued messages stay in the inbox until they are delivered.
- Switching devices in the Dashboard restores the last chat.
- Chat navigation shortcuts follow the sidebar order.
- The chat ruler no longer grows past its maximum height or shows a scrollbar.
- `pontia update` refreshes installed Pi integrations even when Pontia is already up to date.

## [0.7.13] - 2026-11-16

### Changed

- Pontia Edge checks for automatic updates every 10 minutes instead of hourly.

### Fixed

- Pi applies the Agent Profile system prompt on the first prompt of a new Session and after switching Sessions.
- The Dashboard uses the same conversation skeleton everywhere a conversation loads.
- Dashboard progress bars no longer jump when the total becomes known.

## [0.7.12] - 2026-11-16

### Added

- `pontia uninstall` removes Pontia integrations, shell configuration, and autostart services.

### Changed

- Pi Sessions started directly in a terminal connect to Pontia without `tmux`.

### Fixed

- The Dashboard keeps the conversation visible while it reconnects.
- The Public Dashboard shows cached devices while it refreshes them in the background.
- Switching Sessions in the Dashboard restores cached history right away.
- Pi no longer initializes the Pontia extension twice when it is loaded more than once.

## [0.7.11] - 2026-11-15

### Added

- `pontia completions <shell>` prints shell completion scripts, including Client names for `pontia setup`.
- The mobile Dashboard has a new chat button in the top bar.

### Changed

- Claude Code is set up with `pontia setup claude`.

### Fixed

- The Public Dashboard reconnects to device event streams without showing the device as offline.
- Restored chats scroll to the latest message after the Session loads.
- Submitted chat messages stay visible while the transcript loads.
- Codex tool calls display correctly in the Dashboard.

## [0.7.10] - 2026-11-14

### Fixed

- The Public Dashboard keeps its encrypted connection to a device when you reload the page.
- Session titles set by the Client now replace titles taken from the first message.

## [0.7.9] - 2026-11-14

### Added

- Claude and ACP Sessions can switch models in the same conversation with `/model` in the message composer.

### Changed

- Running `pontia init` again now checks and repairs the installation. Later runs default to the enabled Clients, keep the settings of unchecked Clients, and reuse healthy integrations.
- `pontia init`, login, and device registration output is easier to read.
- The Dashboard shows the Pontia mascot with loading and offline states on every page while a device connects or is offline.
- The Cloud site remembers the selected language and opens pages in that language.
- The Cloud site slogan is shown in English for all languages.

### Removed

- Removed the built-in Kimi CLI ACP entry.

### Fixed

- `pontia update` now checks for active tasks before downloading a release.
- Opening and closing the Codex TUI without sending a message no longer creates a Session.
- Codex sessions started through Pontia now open in the directory where `codex` was launched.
- Codex history messages now keep their timestamps.
- The Codex daemon keeps running after its autostart unit exits.
- Messages sent to a resuming Session are now delivered once the Session is ready.
- On the Dashboard's new chat page, you can switch to another device while the current device is still connecting.
- The Dashboard error page now shows a centered "Page not found" message for missing pages and the actual error for other failures.

## [0.7.8] - 2026-11-13

### Changed

- The Dashboard now asks search engines not to index its pages.
- Updated the Codex and OpenCode icons in the Dashboard and on the Cloud site, with an OpenCode icon for the dark theme.

### Fixed

- Pontia now detects device connections that stop responding without closing, instead of leaving them open.
- Signing in from a translated Cloud page, including Account settings, now returns to the page in the same language.
- Chinese and Japanese pages on the Cloud site now use fonts suited to those languages.

## [0.7.7] - 2026-11-13

### Added

- Added `npx @pontia/cli install`, which installs `pontia` and `pontiad` from npm without running `pontia init`.
- Added `pontia client list --json` for the full entries, including IDs and effective configuration.
- The Cloud site is now available in Chinese, Japanese, and Brazilian Portuguese, with a language switcher in the header.

### Changed

- `pontia client list` now prints a table by default.
- `pontia setup [CLIENT]` now enables the selected Clients after the Pontia service starts.
- `pontia --help` now lists agent tools such as `send-file` and `workflow` in their own section.
- `pontia update` now stops and lists the affected Sessions when the running service has active tasks, pending interactions, or pending inputs.
- The install scripts no longer require `jq`.

### Fixed

- Codex threads started, resumed, or forked through the Pontia launcher now have the Pontia session environment, such as `PONTIA_SESSION_ID`, from the first input.

## [0.7.6] - 2026-11-12

### Added

- Added installing Pontia with `npx @pontia/cli init`. `npx @pontia/cli update` runs `pontia update`.
- Added a dark theme to the Dashboard, selectable as System, Light, or Dark in the new Appearance settings.
- Added dark mode to the Cloud site, with a theme selector in the header.
- Codex sessions now use the thread name set in Codex as their title.
- Added blog posts comparing remote-control options for coding agents and on self-hosted relay encryption.

### Changed

- Session titles now prefer a manual rename, then a title from the client, then the first prompt.
- Messages with uploaded files now tell the agent the files are temporary and should be copied into the workspace if needed.
- The documented macOS minimum is now 13.5.

### Removed

- Removed opening Codex sessions in a Pontia-managed Codex TUI, along with the `runtime.codex.tui_command` setting.

### Fixed

- Codex sessions opened through the shared Codex daemon now stop being managed when the Codex client unsubscribes or disconnects.
- Session titles generated from a first prompt with uploaded files now use the prompt text instead of the upload markup.
- Improved Dashboard dark mode contrast for buttons and text.
- The device approval button on the Cloud site is now on the right.

## [0.7.5] - 2026-11-12

### Added

- Added `pontia setup [CLIENT]` to install or repair the recommended integration for pi, Codex, or Claude Agent without changing Client enablement.
- The public Dashboard now guides you through connecting your first device.
- The Cloud site's `llms.txt` now includes blog posts and is linked from the footer.
- Added blog posts on coding agent phone notifications and remote access, and expanded the DIY setup sections of existing posts.

### Changed

- Signed-out users of the public Dashboard are now redirected to sign-in.
- Redesigned the Cloud sign-in page.
- The chat page no longer shows the Agent Profile status label above the timeline; profile errors are still shown.

### Fixed

- Workflow replanner sessions keep their workflow context after a restart or resume.

## [0.7.4] - 2026-11-11

### Added

- Added editing an earlier prompt in Codex sessions. Saving the edit creates an independent session with the history before that prompt and sends the replacement there; the original session is kept.
- Codex threads opened through the shared Codex daemon, including from the Pontia Codex launcher, now appear as Pontia sessions.
- Codex sessions now show context usage and load their history from Codex.
- Added automatic cleanup of uploaded files, configured in the new `[uploads]` section.
- Added a blog to the Cloud site.

### Changed

- Uploaded files now appear above the user message, and pending attachments above the composer input. Upload errors are shown as toasts.
- The chat status bar now shows the agent client icon first and no longer shows the control connection state.
- Agent profile system prompts are now applied once when a session is created.
- Expanded the Cloud docs to cover Claude Agent, ACP agents, and recent Dashboard features.

### Fixed

- An idle session's chat no longer loses its visible history.
- In the public Dashboard, New Chat now opens on the active device.
- Dashboard components now play their open and close animations.

## [0.7.3] - 2026-11-11

### Added

- Added a social share image to the Cloud homepage.

### Changed

- Redesigned the Cloud homepage around an interactive Dashboard demo, with feature, integration progress, open source, and FAQ sections.
- Shortened `pontia` command help text.

### Fixed

- The chat page now shows a single conversation skeleton while a session loads.
- The public Dashboard's device loading screen now matches the connecting screen.

## [0.7.2] - 2026-11-09

### Added

- Added `pontia doctor`, which explains service, configuration, login, and remote-registration checks and suggests repairs.
- Added the signed-in Cloud account and the account's device list to the public Dashboard's common settings.

### Changed

- `pontia status` now shows the data directory, local service health, signed-in user, remote handle, and selected edge. `pontia remote status` has been removed; use `pontia doctor` instead.
- The public Dashboard sidebar now has a device switcher above a single session list, and the selected device applies to the whole Dashboard. Device selectors are hidden when only one device is available.
- Redesigned the workspace browser as a file browser with a toolbar, breadcrumb path, and a sidebar listing active and unavailable workspaces and roots.
- Redesigned settings navigation: settings sections replace the session list in the main sidebar, and phones switch sections from tabs in the top bar. Agent Clients settings now show a client list next to a detail view with an enabled switch and side-by-side override and effective configuration.
- Simplified the connecting and device unavailable screens of the public Dashboard.
- Exiting a Codex session now stops and unloads its thread in the shared Codex daemon.

### Fixed

- Edge updates no longer fail when the restarted service has not yet started the updated executable.
- Workspace availability is now checked by the backend instead of the Dashboard.
- The new chat target selector lists only enabled Agent Clients.

## [0.7.1] - 2026-11-08

### Added

- Added an animated mascot to the Cloud homepage hero.

### Changed

- The public Dashboard now connects to all of an account's devices at the same time. The sidebar lists sessions grouped by device, and the new chat page and settings have a device selector. The separate Devices page has been removed.
- Browser connections to Edge now require a one-time admission ticket issued by Cloud.
- Hid the chat title in the top bar on phone screens, and made the single-line chat composer more compact on phones.
- Improved Cloud documentation navigation on phones, and made wide documentation tables scroll horizontally.

### Fixed

- Queued Inbox messages sent during a manual `/compact` until compaction finishes.
- Kept sent messages visible in Dashboard chats until the agent's own history loads, so the page no longer jumps back to the previous turn.

## [0.7.0] - 2026-11-08

### Added

- Added an optional Pontia Codex launcher to `pontia init`. When enabled, interactive `codex` commands in new shells connect to the Pontia-managed Codex daemon, so those sessions appear in Pontia. Other Codex subcommands run the original executable unchanged.
- Added context compaction checkpoints to Dashboard chats, and a `/compact` command to compact a session's context manually.
- Added message times to Dashboard chats.
- Added renaming a session from its title in the chat top bar.

### Changed

- `pontia init` now prints shell activation instructions when it finishes.
- Shortened the composer placeholders so they fit on phone screens, and reduced the top bar title size.
- Shortened the warning shown when some history cannot be associated with a turn.

### Fixed

- Kept tmux sessions running when the Pontia service restarts or stops under systemd.
- Made chat shortcuts work on the public Dashboard and while the composer is focused, including Option shortcuts on macOS. The shortcuts dialog shows ⌘/⌥ on Apple platforms and Ctrl/Alt elsewhere.
- Showed the duration of failed turns.
- Adapted the Pi extension to Pi 1.5.0, and recorded editing or replaying input in Pi as a new turn.
- Accepted Dashboard release archives that contain a root directory entry.

## [0.6.15] - 2026-11-07

### Changed

- Moved file uploads in Dashboard chats to a + button in the composer, and moved the shared file list into a dialog in the chat actions menu. Files shared during a turn appear as cards at the end of that turn.
- Kept the chat composer on one line while the draft fits, and gave the new chat page the same composer with compact agent client and workspace selectors. Agent clients show their brand icons.
- Made the chat top bar float over the conversation, moved the sidebar toggle next to the Pontia brand, and moved the shortcuts button to the sidebar footer.
- Replaced the always-on connection icon with a toast shown after 3 seconds of lost connection. Toasts now appear at the top center.

### Fixed

- Stopped Inbox panels from flashing when sending a message. Messages sent to an idle session no longer also appear in the Inbox queue, and queued messages are listed in delivery order.
- Showed the configured display names for Pi and Codex in new session options.
- Showed the plain New Chat icon in the collapsed sidebar.

## [0.6.14] - 2026-11-05

### Added

- Added file attachments to Dashboard chats. Uploads are end-to-end encrypted over remote access, resume after interruptions, and support files up to 1 GiB.
- Added `pontia send-file PATH` for agents to share a file with the user of the current session. Shared files appear in the session chat in Dashboard and can be previewed or downloaded, up to 1 GiB.
- Exposed the current Pontia session ID to commands run by Codex, ACP, Claude Agent, and Pi sessions through `PONTIA_SESSION_ID`.
- Added an animated mascot to the Dashboard new session page.

### Fixed

- Kept tmux session bindings working when Pontia runs in the C locale.
- Omitted inline image data from Pi session history so pages with images load within the response size limit.

## [0.6.13] - 2026-11-03

### Changed

- Refreshed the Pontia logo and favicon in the dashboard and on the website.
- Switched the dashboard and website to rounded corners on buttons, inputs, cards, dialogs, menus, and other controls.
- Changed session detail and workspace selector icons in the dashboard to neutral colors.
- Updated the dashboard screenshot on the homepage.

## [0.6.12] - 2026-11-03

### Changed

- Reduced Pontia download sizes by distributing the Claude Agent client separately as `@pontia/claude-client`. Selecting `claude-agent` during `pontia init` downloads the client and SDK dependencies from npm; Node.js LTS, npm, and internet access are required for first setup.
- Reuse the installed Claude Agent client across Pontia updates. The client can be updated separately using the [Claude Agent setup guide](docs/claude-agent.md).

## [0.6.11] - 2026-11-03

### Fixed

- Keep other devices connected when removing a device from the same account.
- Skip updates when the installed Pontia version is newer than the stable release.
- Fix Claude Agent startup when Pontia's data directory has a long path, including on macOS.
- Report Claude Agent startup errors without exposing private paths or credentials.

## [0.6.10] - 2026-10-31

### Changed

- Bundled the Claude Agent host and SDK dependencies with Pontia. Node.js LTS remains required; select `claude-agent` during `pontia init` to check dependencies and enable the integration.
- Raised the minimum macOS version for Pontia's local control plane to 13.5.

### Fixed

- Show authentication and model-request failures while the Claude SDK retries, without reporting a false task result.

## [0.6.9] - 2026-10-31

### Added

- Added Claude Agent session control, streaming output, tool approvals, native history browsing, and resuming closed sessions. Requires a separately installed Claude Agent host.
- Added a quick installation command and Getting started guide on the Account page for users without registered devices.

### Changed

- Highlighted Pontia's open-source positioning on the homepage and moved the GitHub link from the header to the Hero section.

## [0.6.8] - 2026-10-27

### Added

- Added non-interactive remote device registration with `pontia remote enable --edge EDGE_ID --name NAME`.
- Added an initialization guide covering interactive and step-by-step setup.
- Published the documentation as Markdown with an `llms.txt` index.

### Changed

- Expanded `pontia` command help with descriptions, usage notes, examples, and documentation links.

## [0.6.7] - 2026-10-27

### Added

- Added live reasoning from Codex, ACP, and Pi sessions to Dashboard as Thinking steps.

### Changed

- Loaded remote access in the Public Dashboard as a separately deployed module.

### Fixed

- Refreshed Dashboard session views when updates arrive while a session is still loading.

## [0.6.6] - 2026-10-23

### Changed

- Moved Settings to the Dashboard sidebar footer.

### Fixed

- Disabled agent clients without persisted management records for new sessions, including after restoring inherited settings.
- Rejected reserved ACP agent IDs `.` and `..` during configuration and session selection.
- Kept streamed tool output inside its collapsible tool details in Dashboard.

## [0.6.5] - 2026-10-22

### Added

- Added live tool execution status, results, and output in Dashboard for Pi, native Codex, and ACP sessions.
- Added Agent Client management in Dashboard and the `pontia client` CLI, with persistent configuration overrides, enable/disable controls for new sessions, and restoration of inherited settings.

### Changed

- Made initialization enable successfully installed client integrations through the running service.
- Added integration maintenance during updates for clients explicitly enabled in persisted settings.

## [0.6.4] - 2026-10-20

### Added

- Added a Dashboard preview and integration progress indicator to the Cloud homepage.
- Added workspace filtering for recent sessions in the Dashboard sidebar.

### Fixed

- Hid the pinned sessions group when no sessions are pinned.

## [0.6.3] - 2026-10-19

### Added

- Added streaming and recovery of native Codex tool calls.
- Added session waiting-state reporting for ACP permission interactions.

### Fixed

- Made `pontia up` prepare persistent local authentication, verify Dashboard availability, and print an authenticated Dashboard URL.

## [0.6.2] - 2026-10-17

### Added

- Added a session waiting state for blocking interactions, including native Codex approval requests.

### Changed

- Standardized logo stripe spacing and used the full logo in Cloud pages and the Dashboard sidebar.
- Removed the Progress link from the Cloud site header.

### Fixed

- Preserved the device connection when navigating between Dashboard pages for the same device.

## [0.6.1] - 2026-10-17

### Changed

- Hid the Workflows entry from the Dashboard sidebar.

### Fixed

- Read offline Pi session entry history from persisted JSONL files.
- Improved logo rendering at small sizes and on dark backgrounds.

## [0.6.0] - 2026-10-17

### Added

- Added ACP agent integration with registry and local command launches, agent selection, permission requests, conversation history, and bound-session recovery.
- Added local ACP defaults for Grok Build, Kimi CLI, and OpenCode with per-agent configuration overrides.
- Added shared session interactions and Codex approval controls for commands, file changes, and additional permissions.
- Added a chat actions menu beside the session status in Dashboard.
- Added a client integration capability overview to the Cloud homepage.

### Changed

- Simplified SQLite validation by removing duplicate domain checks and defensive triggers.

### Fixed

- Made Pi turn-start reporting idempotent so retries do not create duplicate turns.
- Showed chat creation and interaction errors as transient toasts in Dashboard.
- Hid chat actions on the new chat page and adjusted agent state indicator colors.
- Declared explicit client control roles and replaced stale readiness metadata on reconnect.

## [0.5.3] - 2026-10-13

### Added

- Added context compaction controls, events, and history for Pi and Codex sessions.
- Added remote device status and profile management.
- Added hosted privacy and terms pages and expanded the public documentation.

### Changed

- Recorded authentication session activity and request details in Cloud.
- Simplified Cloud session and device revocation.

### Fixed

- Matched Dashboard tool results to calls by their stable identities.

## [0.5.2] - 2026-10-12

### Added

- Added Cloud controls for editing device names and handles.
- Added live Codex assistant output through the shared conversation streaming path.

### Changed

- Completed the Dashboard migration of profiles, workspaces, sessions, Inbox, timelines, and public device data to TanStack Query.
- Moved Pi entry history recovery to a bounded runtime RPC.
- Published installer scripts directly from the repository release workflow.

### Removed

- Removed the obsolete message update event and its volatile event broker path.

## [0.5.1] - 2026-10-10

### Added

- Added a unified Sessions view with active and archived tabs backed by a dedicated session overview query.
- Added the Edge initialization retry command to failed deployment instructions in Cloud.

### Changed

- Migrated Dashboard session, workspace, and workflow reads and invalidation to TanStack Query.
- Aligned the Dashboard settings navigation with Cloud.
- Unified macOS service executable discovery across supported installation locations.

### Fixed

- Preserved session activity timestamps when management metadata changes.

## [0.5.0] - 2026-10-10

### Added

- Added macOS support for running, installing, and updating the Pontia control plane.
- Added live health status to Edge deployment settings in Cloud.
- Added a sitemap for public Cloud pages.

### Changed

- Moved Agent Client setup, profiles, lifecycle handling, configuration, and Dashboard behavior behind client-owned contracts.
- Centralized Agent Client registration and background service startup.

### Fixed

- Showed total chat duration in completed work summaries.
- Limited queued-message displays to pending and failed Inbox messages.
- Restricted platform-specific service helpers to supported operating systems.
- Verified the local Edge process after updates before reporting success.

## [0.4.3] - 2026-10-09

### Added

- Added live and completed turn durations to Dashboard conversations.

### Changed

- Unified binaries, the Pi client, and Cloudflare applications under the root product version and a single production release workflow.

### Fixed

- Accepted URL-safe credential secrets containing underscores when Edge connects to Cloud.
- Hid injected Pi skill instructions from Dashboard conversation messages and generated session titles.
- Allowed managed Codex threads to receive their first input and recover before Codex materializes their rollout history.

## [0.4.2] - 2026-10-09

### Added

- Added Cloud settings for managing sign-in methods, registered devices, browser and CLI sessions, and Edge deployments.
- Added Cloud documentation for setup, remote access, and Pontia's architecture.
- Added resumable Edge initialization with `pontia-edge retry`.

### Changed

- Redesigned the Cloud landing page and added direct entry points to the Dashboard and documentation.
- Simplified `pontia init` by using the home directory for first-time workspace access and preserving existing roots without prompting.
- Reused Cloudflare build artifacts across deployment jobs and prerendered Cloud documentation routes.

### Fixed

- Made the Edge administration script directly executable.

## [0.4.1] - 2026-10-08

### Added

- Added automatic stable release updates for Edge deployments.
- Added explicit runtime lifecycle events and stable session runtime identities across Agent Client integrations.
- Restored native Codex thread integration and added managed Codex TUI runtime lifecycle support.

### Changed

- Consolidated Public Dashboard device connection bootstrap into a single capability exchange.
- Updated the Pi extension to use stable session runtimes.

### Fixed

- Closed Codex subscription lifecycle gaps during session and Workflow execution.
- Waited for workspace loading to settle before showing new-chat onboarding.

## [0.4.0] - 2026-10-06

### Added

- Added end-to-end encryption between the Public Dashboard and devices, backed by a shared native/WASM protocol implementation with authenticated session establishment, replay protection, and encrypted streaming responses.
- Added Cloud-issued short-lived E2E capabilities, proof-of-possession device registration, and persistent device encryption identities.
- Added `pontia remote rotate-key` for replacing a device encryption identity.

### Changed

- Routed local token-authenticated and remotely E2E-authenticated requests through the same External API router and shared 2 MiB request-body policy.
- Restricted Edge and device tunnels to the fixed encrypted session and request transports; Edge now relays ciphertext without interpreting Dashboard API traffic.
- Generated the Public Dashboard WASM module during Cloudflare deployment instead of storing generated bindings in Git.

### Fixed

- Enforced expiry when authenticating CLI credentials.
- Authenticated complete bounded E2E requests before dispatching them to business handlers or refreshing session activity.

### Removed

- Removed plaintext Dashboard access tickets, browser authorization cookies, Edge SQLite browser authorization state, and the trusted-tunnel authentication bypass.

## [0.3.6] - 2026-10-03

### Changed

- Used system-configured DNS servers for Edge certificate propagation checks instead of a fixed external DoH service.
- Bounded Edge DNS propagation waits to five minutes and reported progress every 15 seconds.
- Added explicit timeouts and progress reporting for Edge Cloud and ACME requests.

### Fixed

- Hardened Edge initialization cancellation with SIGINT/SIGTERM handlers installed before initialization and cancellable systemctl calls.
- Cleaned up shared HTTP-01 challenge tokens after certificate issuance timeouts.

## [0.3.5] - 2026-10-03

### Added

- Added `pontia-edge update` to install verified, signed stable releases.
- Added owned Edge listings and a deployment command page in Cloud.

### Changed

- Removed the explicit Let's Encrypt subscriber agreement flag from Edge initialization.

### Fixed

- Discovered Edge public IPv4 addresses through Cloud to support NAT deployments.
- Allowed localhost HTTP authentication in Cloud development environments.

## [0.3.4] - 2026-10-03

### Added

- Added `pontia update` to install verified, signed stable releases.
- Added signed release manifests and Linux installers distributed through R2.
- Added custom Edge ports and DNS-01 certificate issuance.
- Added guided remote-access setup during initialization and a command to disable remote access.

### Changed

- Updated the Pi client integration for Pi 1.0.
- Moved generated device names to private vocabulary tables.

### Fixed

- Kept active Cloud browser sessions signed in with sliding session refresh.

## [0.3.3] - 2026-10-01

### Added

- Added scheduled cleanup for expired Edge connection tickets and their DNS records.
- Added clearer progress reporting to the Edge deployment console.

### Fixed

- Stabilized Edge DNS configuration and improved Cloudflare API error handling.
- Allowed public Cloudflare API requests from Cloud and the cleanup worker.
- Accepted underscores in CLI login credentials.
- Fixed the Dashboard device bootstrap form flow.

## [0.3.2] - 2026-10-01

### Fixed

- Deferred self-hosted Edge registration until its public HTTPS health endpoint has been verified.
- Verified Edge public IPv4 control through direct HTTP probes without following redirects.

## [0.3.1] - 2026-09-30

### Fixed

- Built Linux release binaries on Ubuntu 22.04 to support systems with glibc 2.35 or newer, including Debian 12.

## [0.3.0] - 2026-09-30

### Added

- Added self-hosted Edge enrollment, authenticated device tunnels, one-time connection tickets, and automated Edge networking.
- Added Cloud account linking, device authorization, and remote Dashboard access through registered Edge devices.
- Added an experimental native Codex integration with conversation history, runtime control, model selection, profile binding, and daemon management.
- Added live turn output, richer tool-use rendering, file mentions, chat slash commands, and archived-session recovery to the Dashboard.

### Changed

- Moved Agent Client integrations behind dedicated adapters and application services, with Pi control and event reporting using bidirectional Unix JSON-RPC.
- Redesigned the Dashboard and split local and public operating modes behind explicit build boundaries.
- Renamed the hosted website domain to Cloud and migrated its package tooling to Bun.
- Unified business HTTP endpoints under `/api/v1` and simplified native session control and history recovery.

### Fixed

- Added durable Workflow retries when Pi nodes fail to launch or report their initial turn.
- Improved Pi and Codex reconnection, native history recovery, turn-boundary restoration, and Inbox delivery retry handling.
- Hardened Cloud and Edge authorization, device login, deployment configuration, and tunnel authentication.

## [0.2.1] - 2026-09-11

### Added

- Added an Active Workspaces dialog to the Dashboard.

### Changed

- Redesigned Workflow history around direct revision selection, per-phase historical views, and replanning records grouped by revision.
- Simplified Workflow file handoffs by giving agents exact input, output, and problem-report paths and removing file arguments from submission and patch commands.
- Clarified Workflow worker completion and replanning instructions.
- Simplified the product and Pi plugin setup documentation.
- Automated Pi plugin releases through npm Trusted Publishing.

### Fixed

- Bounded persisted turn input summaries to prevent large Workflow prompts from exceeding event payload limits.
- Made turn-start reporting failures fail affected Workflows instead of leaving submitted nodes waiting indefinitely.

## [0.2.0] - 2026-09-09

### Added

- Added Workflow pause/resume controls, recoverable coordination, and definition revision history.
- Added Workflow patch requests, replanner sessions, patch application/rejection, and side-effect recovery.
- Added Dashboard views for Workflow revisions, patch timelines, and replanning history.
- Added guided workspace setup, improved directory browsing, and a queued messages panel.

### Changed

- Local setup now installs the local Pi plugin, and `pontia up` restarts an already-running service.

### Fixed

- Improved Workflow turn correlation and monitor wait stability.
- Fixed Pi interruption reporting and excluded ephemeral and headless sessions from tracking.
- Fixed initialization bind address defaults, service startup failure handling, and runtime binding migration preservation.

### Removed

- Removed the obsolete Claude Code integration; Pi remains the supported agent client.

## [0.1.0] - 2026-08-20

Pontia's first public preview establishes a local control plane for long-lived coding-agent sessions.

### Added

- Added the `pontia` lifecycle CLI and `pontiad` Control Plane daemon, with interactive setup and per-user service management on Linux and macOS.
- Added the first-party Pi integration for controlling real, tmux-backed TUI sessions from either the terminal or Web Dashboard.
- Added Dashboard support for creating, viewing, resuming, interrupting, and terminating sessions across configured workspaces.
- Added native conversation history, branching and replay, queued messages, context usage, file mentions, and Git status visibility.
- Added experimental linear Workflow execution through the CLI and Dashboard.

### Known limitations

- Pontia is experimental and currently supports Pi as its only active agent-client integration.
- Agent-planned WorkItem DAG orchestration is not included in this release.

[Unreleased]: https://github.com/anthod0/pontia/compare/v0.6.9...HEAD
[0.6.9]: https://github.com/anthod0/pontia/compare/v0.6.8...v0.6.9
[0.6.8]: https://github.com/anthod0/pontia/compare/v0.6.7...v0.6.8
[0.6.7]: https://github.com/anthod0/pontia/compare/v0.6.6...v0.6.7
[0.6.6]: https://github.com/anthod0/pontia/compare/v0.6.5...v0.6.6
[0.6.5]: https://github.com/anthod0/pontia/compare/v0.6.4...v0.6.5
[0.4.0]: https://github.com/anthod0/pontia/compare/v0.3.6...v0.4.0
[0.3.6]: https://github.com/anthod0/pontia/compare/v0.3.5...v0.3.6
[0.3.5]: https://github.com/anthod0/pontia/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/anthod0/pontia/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/anthod0/pontia/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/anthod0/pontia/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/anthod0/pontia/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/anthod0/pontia/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/anthod0/pontia/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/anthod0/pontia/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/anthod0/pontia/releases/tag/v0.1.0
