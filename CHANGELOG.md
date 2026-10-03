# Changelog

All notable changes to Pontia will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/anthod0/pontia/compare/v0.3.4...HEAD
[0.3.4]: https://github.com/anthod0/pontia/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/anthod0/pontia/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/anthod0/pontia/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/anthod0/pontia/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/anthod0/pontia/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/anthod0/pontia/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/anthod0/pontia/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/anthod0/pontia/releases/tag/v0.1.0
