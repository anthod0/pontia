# Issue tracker: Local Markdown

Issues and specs (you may know a spec as a PRD) for this repo live as markdown files in `../pontia-docs/scratch/`.

## Conventions

- One feature per directory: `../pontia-docs/scratch/<feature-slug>/`
- The spec is `../pontia-docs/scratch/<feature-slug>/spec.md`
- Implementation issues are one file per ticket at `../pontia-docs/scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01` — never a single combined tickets file
- Triage state is recorded as a `Status:` line near the top of each issue file (see `triage-labels.md` for the role strings)
- Comments and conversation history append to the bottom of the file under a `## Comments` heading

## When a skill says "publish to the issue tracker"

Create a new file under `../pontia-docs/scratch/<feature-slug>/` (creating the directory if needed).

## When a skill says "fetch the relevant ticket"

Read the file at the referenced path. The user will normally pass the path or the issue number directly.
