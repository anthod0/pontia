# Changelog

All notable changes to Pontia will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/anthod0/pontia/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/anthod0/pontia/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/anthod0/pontia/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/anthod0/pontia/releases/tag/v0.1.0
