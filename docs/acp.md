# ACP Sessions

Create a Session with an enabled ACP Agent from the Dashboard's client selector.

Choose a model before starting when the Agent provides model options. Leaving the model at Default uses the Agent's default for the selected Workspace. Some Agents create an empty conversation while checking available models; it may remain in their history.

Press Enter to send a follow-up to the current task when the Agent offers steering. Otherwise, the message waits in Inbox until the task finishes. Tab or Alt+Enter always queues the message for after the current task.

When the Agent reports its current model, Dashboard shows it in the Session details.

Agents offering system prompt support receive Pontia's built-in instructions, your selected Profile's static system instructions, and any `additional_instructions` supplied when creating the Session. These instructions supplement the Agent's own instructions.

If the Agent lacks this capability, Sessions using only Pontia's built-in instructions can still start, with instruction injection disabled. Explicit Profile instructions or `additional_instructions` require the capability; creation fails if the Agent cannot apply them. Session details show whether injection is enabled.

Resuming a Session preserves its original instructions and whether injection was enabled. Profile edits apply to new Sessions.
