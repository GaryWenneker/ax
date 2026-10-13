# Pipeline suspension

Automatic CI, release and deployment workflow execution is paused at the owner’s request on 2026-10-11. On 2026-10-13 the owner explicitly requested ongoing dependency lifecycle security checks; the new `dependency-policy.yml` (pull requests) and `dependency-monitor.yml` (daily schedule) are narrow, security-only exceptions. The suspended workflows retain `workflow_dispatch` so their definitions remain available. They do not receive new automatic push, pull request, workflow completion, schedule or reusable-workflow triggers. Do not manually dispatch them during code stabilization without a new owner instruction.

Local tests and builds remain available. Existing successful runs are historical evidence, not checks of subsequent changes. Releases and deployment are separate from merging code.

To resume, the owner must explicitly request it; restore the relevant event blocks from the commit before this suspension, review their path filters and dependent release triggers, then validate the workflow YAML. Do not re-enable automatic release publication just to run a code check.
