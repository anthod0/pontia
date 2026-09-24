# Codex Agent Profiles

Codex Profile bindings use a fixed execution contract for a Pontia Session. A
Profile ID and version alone do not establish that native instructions were
configured.

| Field | Codex contract |
| --- | --- |
| `system_prompt_template` | Optional, nonblank static text. `{{…}}` placeholders are rejected. Passed as native developer instructions, above user input and below native system instructions. |
| `turn_prompt_template` | Unsupported, including an empty string. Omit it or use JSON `null`. |

A Profile must explicitly include `codex` in `supported_client_types`. Invalid
Codex templates are rejected when creating or editing the Profile and again when
binding a Session, including Profiles stored before validation was introduced.
Missing, archived, incompatible, and unknown-version Profiles cannot be bound.
These restrictions do not change Pi's Profile behavior.

When the API creates a Session with `execution_profile_id`, an omitted version
resolves to the latest active version at that time. The binding captures the
version and instruction content. Editing or archiving the Profile afterward does
not change existing Sessions. Create a new Session to use different instructions.

The first real input creates the native thread and configures the instructions.
Subsequent Dashboard inputs and the Pontia-managed native TUI use that same
thread. Resuming through Pontia reapplies the fixed configuration as a value,
without concatenating it with earlier copies. User messages remain unchanged.
Conflicting developer-instruction overrides on a managed TUI resume are rejected.
A TUI switch to another thread resolves that thread's own binding.

The mapping uses Codex `developerInstructions`, preserving native system
instructions. It occupies the native developer-instruction configuration slot;
it does not merge an additional user-configured `developer_instructions` value.
Native tools, approvals, project instructions, and model behavior remain subject
to Codex. Clients that bypass Pontia's TUI gateway can change native configuration
outside this contract.

The Session API's `codex.profile` and Dashboard display one of these states:

- `awaiting_input`: binding content is fixed; no native thread has been configured.
- `configured`: the native thread creation request with the configuration was
  accepted. This is configuration evidence, not a claim that every future model
  answer will follow the instructions.
- `unverified`: the binding lacks a supported snapshot or native configuration
  record. Pontia input, resume, and managed TUI execution are rejected with a
  reason. Existing instructions are not silently changed; create a new Session.

External threads imported through the TUI do not automatically acquire a
Profile. Profile selection in New Chat and Codex Workflow support are outside
this contract.

The adapter targets Codex 0.156.1. The opt-in native acceptance test
`native_profile_controls_dashboard_tui_and_resumed_thread` requires an externally
running daemon with model access. It checks a marker present only in the Profile,
real TUI input, cold resume of the same thread, and native developer-message
history. HTTP contract tests separately cover the Profile and Session APIs.
