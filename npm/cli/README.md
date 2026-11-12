# @pontia/cli

Install [Pontia](https://pontia.dev/) from the npm registry. Pontia lets you run and control your coding agents (pi, Codex, Claude Code, and ACP agents) from a local or remote Dashboard.

The `pontia` and `pontiad` binaries come from a platform package on your configured registry, so npm mirrors and private registries work.

## Requirements

- Linux with systemd, or macOS 13.5 or later, on x64 or arm64.
- Node.js with npm. Do not install with `--omit=optional`.

## Usage

Install Pontia and start the interactive initializer:

```sh
npx @pontia/cli init
```

Install Pontia without initializing, for example when an agent or script sets up the device step by step:

```sh
npx @pontia/cli install
```

Both commands place `pontia` and `pontiad` in `$HOME/.local/bin`. Set `PONTIA_INSTALL_DIR` to use a different directory, and make sure the directory is in your `PATH`. If both binaries are already installed, they are kept.

Update to the latest signed release:

```sh
npx @pontia/cli update
```

This runs `pontia update`, which downloads from the Pontia release server instead of npm.

## Documentation

- [Getting started](https://pontia.dev/docs/getting-started)
- [Initialization](https://pontia.dev/docs/initialization)

## License

Apache-2.0
