# Branch rulesets

Reviewable copies of the GitHub rulesets the delivery process relies on. The managing agent may apply settings after reviewing them under the owner's standing authorization. Files do not apply themselves; `"enforcement": "active"` specifies the intended setting when applied, not deployment status.

## Status

- `milestone.json`: applied and read back on 2026-09-15 as ruleset `23480411`. Protects `milestone/*` umbrella branches with no force-pushes, rebase-only pull requests, and the full-gate checks (`verify-full` and the four supported-target checks) on an up-to-date branch. Creation from the protected main revision does not require checks on the new ref; subsequent updates require PRs and checks. Rebase-only PRs keep new integration linear; a whole-history linearity rule would reject merge commits inherited from main when creating an umbrella. Deletion remains available for retired umbrellas.
- `stable.json`: applied and read back on 2026-09-15 as ruleset `23480413`. Blocks deletion of `stable` without bypass actors. The branch and release workflow do not exist yet. Owner authorization, the full gate, and the security checks in [SECURITY.md](../../../SECURITY.md) are separate prerequisites for a stable push; this ruleset does not implement them.
- `main`: active ruleset; unchanged here and not copied here. Blocks deletion and force-pushes, requires merge-commit-only pull requests, `verify-full` and the four target checks, and disables the up-to-date ancestry requirement. Recheck the live settings before integration.

On 2026-09-15, the `staging` branch and its ruleset `22294283` were retired after the open dependency updates were rehomed separately, without merging them.

On 2026-09-15 a disposable umbrella was created from the protected main revision. A subsequent direct update was rejected for bypassing a pull request and all five required checks; the disposable branch was then deleted.

## Apply and verify

Inspect live settings and compare the proposed file before applying it. The [GitHub rulesets API](https://docs.github.com/en/rest/repos/rules#create-a-repository-ruleset) requires repository administration write permission. For a new ruleset:

```sh
gh api --method POST repos/mdml/ask/rulesets --input docs/development/rulesets/milestone.json
gh api --method POST repos/mdml/ask/rulesets --input docs/development/rulesets/stable.json
```

For an existing ruleset, use `PUT repos/mdml/ask/rulesets/<id>` with the corresponding file instead of creating a duplicate. Read it back with `gh api repos/mdml/ask/rulesets/<id>` and compare `enforcement`, `bypass_actors`, `rules`, and `conditions`. Record the ruleset ID and application date here only after that comparison passes. Apply stable deletion protection before creating the branch for the first authorized release.
