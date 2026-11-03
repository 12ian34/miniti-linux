# Apple 3.7.0 hardware, activity and recording-outcome handoff

Local tracking, 2026-11-03. No Linux release or new hardware/local-transcription producer is implied.

- Mac `device_capabilities` uses the existing managed, enrolled, Share diagnostics pipeline. The server retains only coarse validated fields in a latest snapshot, expires it 30 days after observation and removes it on installation revocation/account deletion. No meeting/session identity is attached. Linux remains unknown in this Mac-specific sample.
- Backend analytics canonicalize installation casing at write/read time, sum historical numeric contributions and normalize daily/period activity. Admin metadata uses latest-seen evidence with unknown ties; billing records and financial counters are unchanged. Linux request bodies/auth/usage routes remain compatible.
- Apple recording outcomes distinguish lifecycle from whether new words arrived. Earlier saved words do not satisfy a resumed recording. The informational `recording_transcription_outcome` vocabulary is already in reporting v1; no new email policy or Linux producer is introduced. Future Linux parity must preserve managed/consent/private gates and keep content out.

See [reporting contract](../../miniti/docs/reference/recording-reporting-contract.md), [speaker/hardware evidence](../../miniti/docs/plans/local-speakers-default.md#local-hardware-diagnostics-2026-11-03), and [reliability follow-through](../../miniti/docs/plans/recording-reliability-followup.md#local-activity-and-outcome-follow-through-2026-11-03).
