# changelog style

Rules for `CHANGELOG.md` (mirrors the macOS repo's conventions):

- Write entries as human-readable descriptions for a public audience. No code references, function names, file paths, or implementation details. Write what changed from the user's perspective.
- Every bullet starts with `new:`, `improvement:`, or `fix:`. No category sub-headings; the prefix is the category.
- Never modify older entries after they are written unless a historical correction is explicitly requested. Add corrections, clarifications, or reversals only as a new entry at the top.
- While a release is being prepared, its top entry may use `### unreleased - vX.Y.Z`. Replace `unreleased` with the ship date only when the release actually goes out.
- Release headers contain only the date and version. Linux versions are independent of the macOS and iOS version numbers.
- Headings are lowercase, like everything else miniti writes.
- The GitHub Release notes for a tag are copied from that version's entry.
