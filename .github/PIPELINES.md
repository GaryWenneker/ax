# Pipeline suspension

Automatic GitHub Actions execution is paused at the owner’s request on 2026-10-11. All six workflows retain `workflow_dispatch` so their definitions remain available. No push, pull request, workflow completion, schedule or reusable-workflow trigger starts them. Do not manually dispatch them during code stabilization without a new owner instruction.

Local tests and builds remain available. Existing successful runs are historical evidence, not checks of subsequent changes. Releases and deployment are separate from merging code.

To resume, the owner must explicitly request it; restore the relevant event blocks from the commit before this suspension, review their path filters and dependent release triggers, then validate the workflow YAML. Do not re-enable automatic release publication just to run a code check.
