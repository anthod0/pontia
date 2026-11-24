<div align="center">
  <img src="assets/brand/logo-idle.svg" width="80" alt="Pontia">
  <h1>Pontia</h1>
  <p><strong>A Lead Bot for the projects on your machines.</strong></p>
  <p><a href="https://pontia.dev">Website</a> · <a href="https://app.pontia.dev">Open Dashboard</a> · <a href="README.zh-CN.md">简体中文</a></p>
</div>

Give your Lead Bot a project. It can send helpers to work in parallel, bring back their files, and keep the decisions that need you in Todo. Open the Dashboard in a phone or desktop browser to pick up the conversation or switch to another registered machine.

```sh
npx @pontia/cli init
```

Run this on the machine with your signed-in coding CLI. Setup registers the device and opens the Dashboard. Sign in to the browser with your Pontia account and skip chat-bot token setup. [Setup requirements](#requirements)

**Who can read a remote session?** Your browser and the selected device are the endpoints for end-to-end encryption. Pontia Cloud handles sign-in and authorizes the connection. The Edge is a zero-knowledge relay for Dashboard content. It sees ciphertext, routing metadata, traffic size and timing. Choose an official Edge or host a private Edge yourself. Agent tools run with their configured permissions on your machine. [Security model](https://pontia.dev/docs/security) · [Self-host an Edge](https://pontia.dev/docs/self-hosted-edge)

![Juniper assigns two helpers to a field guide, returns the map audit and tracks two decisions.](https://files.pontia.dev/cloud/lead-desk-2640-f11325427ccd.webp)

## One field guide, two helpers

**You, on Friday** — “Get the Luma field guide ready for Saturday. Who can check the trail maps and the pocket edition?”

**Juniper, your Lead Bot** — “The map reviewer is checking junction names in Codex. The print editor is proofing page references in Claude Code.” Each helper has its own task and conversation. When the map reviewer finds two names for the same trail, Juniper puts the choice in Todo and delivers the audit while the editor finishes the index.

**You, on your phone** — Open the report, reply “Use Cedar Spur” on the naming Todo, and leave the winter detour for later. Next time you ask about the guide, Juniper can recall the trail decision and read your standing rule in `USER.md` to give distances in kilometres.

## What the team shares

| Scope | What you can use |
| --- | --- |
| Lead Bot | A continuing conversation, its helpers, their reports and a queue of decisions you can answer individually. |
| Device memory | Bots on the same device share a cross-session memory graph linking facts, decisions and lessons. Browse entries in Settings. Edit personal rules and preferences in `USER.md`. |
| Agent Profiles | Save a role’s instructions, client, model and reasoning effort. Reuse that profile for helpers and workflow steps. Choose among your signed-in Claude Code, Codex, Pi, Google Antigravity, Kiro and Grok Build clients. |
| Remote access | Switch between your registered machines in one browser. Each runs its own clients and keeps its workspaces and files locally. |

<table>
  <tr>
    <td width="50%" valign="top"><a href="https://files.pontia.dev/cloud/shared-memory-2640-36fb6dde6684.webp"><img src="https://files.pontia.dev/cloud/shared-memory-2640-36fb6dde6684.webp" alt="Find a past trail decision in Memory"></a><br><strong>Find a past trail decision in Memory</strong></td>
    <td width="50%" valign="top"><a href="https://files.pontia.dev/cloud/role-profiles-2640-7186bd2133d8.webp"><img src="https://files.pontia.dev/cloud/role-profiles-2640-7186bd2133d8.webp" alt="Set the field researcher’s model and reasoning effort"></a><br><strong>Set the field researcher’s model and reasoning effort</strong></td>
  </tr>
  <tr>
    <td width="50%" valign="top"><a href="https://files.pontia.dev/cloud/private-edge-2640-69ad5bcacac0.webp"><img src="https://files.pontia.dev/cloud/private-edge-2640-69ad5bcacac0.webp" alt="Check the health of a private Edge"></a><br><strong>Check the health of a private Edge</strong></td>
    <td width="50%" valign="top"><a href="https://files.pontia.dev/cloud/helper-decisions-2640-303231730ebb.webp"><img src="https://files.pontia.dev/cloud/helper-decisions-2640-303231730ebb.webp" alt="Reply to a Todo while the print editor works"></a><br><strong>Reply to a Todo while the print editor works</strong></td>
  </tr>
</table>

### On your phone

<p align="center">
  <a href="https://files.pontia.dev/cloud/phone-report-780-89961b4cba82.webp"><img src="https://files.pontia.dev/cloud/phone-report-780-89961b4cba82.webp" width="30%" alt="Receive the map audit in chat"></a>
  <a href="https://files.pontia.dev/cloud/phone-decisions-780-0965988bfd21.webp"><img src="https://files.pontia.dev/cloud/phone-decisions-780-0965988bfd21.webp" width="30%" alt="Answer decisions in the Todo queue"></a>
  <a href="https://files.pontia.dev/cloud/phone-devices-780-ed4abb0a446f.webp"><img src="https://files.pontia.dev/cloud/phone-devices-780-ed4abb0a446f.webp" width="30%" alt="Switch devices and check on the lead"></a>
</p>

## Quick start

### Requirements

- x86_64 and ARM64 Linux, or macOS 13.5 and later on Intel and Apple Silicon.
- a systemd user service manager on Linux; a per-user LaunchAgent on macOS.
- Node.js with npm for the recommended `npx` command. Without Node.js, use the [shell installer](#install-with-the-shell-script).

Install the clients you plan to use before initialization:

- **pi:** the `pi` CLI and `tmux`;
- **Codex:** the `codex` CLI;
- **Claude Code:** Node.js LTS with npm, and a signed-in Claude Code;
- **Grok Build (ACP):** `grok` on the service user’s PATH, enabled after initialization as described in [Agent Clients](https://pontia.dev/docs/agent-clients#enable-an-acp-agent);
- **Google Antigravity (ACP):** `agy` on the service user’s PATH, enabled after initialization in [Agent Clients](https://pontia.dev/docs/agent-clients);
- **Kiro (ACP):** `kiro-cli` on the service user’s PATH, signed in with `kiro-cli login`, and enabled after initialization in [Agent Clients](https://pontia.dev/docs/agent-clients).

### Initialize

`pontia init` installs the selected client integrations, starts the local service and guides you through sign-in and device registration. Add `$HOME/.local/bin` to your `PATH` to run `pontia init` again to check or repair the installation.

### Common commands

Check whether Pontia is working:

```bash
pontia status
```

It shows the service health, the signed-in user, and the remote registration and edge of this device.

Configure or repair a client connection, for example after installing a new client or when a client stops connecting:

```bash
pontia setup          # select clients
pontia setup codex    # or name one: pi, codex, claude
```

When something is wrong, run a detailed check:

```bash
pontia doctor
```

`pontia doctor` checks the service, configuration, login, and remote registration. Each failed check includes a repair hint. For a registered device, the report also shows its remote dashboard address. See [Troubleshooting](https://pontia.dev/docs/troubleshooting) for more help.

### Install with the shell script

You can also use the signed release installer. It requires `curl`, an OpenSSL implementation with Ed25519 support, and standard platform command-line tools.

```bash
curl -fsSL https://get.pontia.dev/install.sh | sh
"$HOME/.local/bin/pontia" init
```

## Manual configuration

`pontia init` covers everything above. Use the following commands to run one step on its own, or to set up without prompts from a script or agent. See [Initialization](https://pontia.dev/docs/initialization#non-interactive-initialization) for the full step-by-step sequence and the [CLI reference](https://pontia.dev/docs/cli) for all commands.

| Command | Purpose |
| --- | --- |
| `pontia up` / `pontia down` | Start or stop the Pontia service |
| `pontia login` | Sign in to Pontia Cloud |
| `pontia remote enable` | Register this device and enable remote access |
| `pontia update` | Update the CLI and service to the latest signed stable release |

### Shell completion

Add the line for your shell, then open a new terminal:

```bash
# bash: ~/.bashrc
source <(pontia completions bash)

# zsh: ~/.zshrc, after compinit
source <(pontia completions zsh)

# fish: run once
mkdir -p ~/.config/fish/completions
pontia completions fish > ~/.config/fish/completions/pontia.fish
```

Elvish and PowerShell are also supported. See the [CLI reference](https://pontia.dev/docs/cli).

### Local dashboard

Open the local dashboard on the device running Pontia at <http://127.0.0.1:8080/dashboard>. Its access token is `external_api_token` in `$HOME/.pontia/config.toml`, generated by `pontia init`.

### Configuration directory

Pontia stores its configuration in `$HOME/.pontia/config.toml` by default. To use another directory, set `PONTIA_HOME` to an absolute path before running `pontia init` and later commands.

## License

Pontia is licensed under the [Apache License 2.0](LICENSE).
