# Session instructions

When creating a Session with `POST /api/v1/sessions`, pass `additional_instructions` to add instructions for that Session. Pi, Codex, and Claude Code accept this field.

```json
{
  "client_type": "codex",
  "workspace": "/path/to/registered/workspace",
  "execution_profile_id": "reviewer",
  "execution_profile_version": "1",
  "additional_instructions": "Focus on authentication changes. Write your findings in Chinese."
}
```

Instructions appear in this order: the Builtin Profile, the selected Profile, then `additional_instructions`, separated by blank lines. Omit the Profile ID and version to use the Builtin Profile with your additional instructions. Custom Profile selection is available for Pi and Codex.

The additional string is used verbatim, including line breaks and braces; it is not a template. Omit it or use `null` when no additional instructions are needed. Empty or whitespace-only strings are rejected, as are requests to inject instructions into clients that cannot accept them.

Instructions remain fixed for the lifetime of the Session, including restart and resume. Editing a Profile affects newly created Sessions. Additional instructions do not change the reusable Profile or other Sessions.
