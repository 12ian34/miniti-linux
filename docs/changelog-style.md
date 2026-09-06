# changelog style

Rules for `CHANGELOG.md` (mirrors the macOS repo's conventions):

- Write entries as human-readable descriptions for a public audience. No code references, function names, file paths, build flags, or implementation details. Write what changed from the user's perspective, in one plain sentence.
- If a user would not notice the change while using the app, leave it out. CI, packaging internals, refactors, and diagnostics belong in commit messages, not here. A release with nothing user-visible gets one line saying what it fixes.
- Keep each release short: the fewer, clearer bullets the better.
- Every bullet starts with `new:`, `improvement:`, or `fix:`. No category sub-headings; the prefix is the category.
- Never modify older entries after they are written unless a historical correction is explicitly requested. Add corrections, clarifications, or reversals only as a new entry at the top.
- While a release is being prepared, its top entry may use `### unreleased - vX.Y.Z`. Replace `unreleased` with the ship date only when the release actually goes out.
- Release headers contain only the date and version. Linux versions are independent of the macOS and iOS version numbers.
- Headings are lowercase, like everything else miniti writes.
- The GitHub Release notes for a tag are copied from that version's entry.
