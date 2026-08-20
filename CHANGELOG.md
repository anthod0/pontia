# Changelog

All notable changes to Pontia will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/anthod0/pontia/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/anthod0/pontia/releases/tag/v0.1.0
