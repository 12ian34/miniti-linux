# Apple local-speaker failure handling, 3.7.0 handoff

2026-11-03, implemented locally in Apple, not released. Linux has no new model or speaker-failure producer from this work. The [Apple plan](../../miniti/docs/plans/local-speakers-default.md#local-failure-handling-implementation-2026-11-03) owns scope and physical-validation limits.

Cloud transcription retains provider separation when all requested local speaker models are not ready before start. After a local failure, Apple bounds queued work, invalidates voice-learning evidence, displays unavailable separation and keeps recording/transcription where possible. A failed timeline stays latched through ordinary transport recovery; no speaker-only reconnect or automatic local-to-cloud upload is added. Mid-session speaker restoration remains assigned to the following release.

Unknown words get a fresh ordinary nonnegative app speaker ID for each source. Apple saves `Unknown speaker` in the existing `speaker_names` map, with no new meeting-document field or reserved numeric ID. Other clients should preserve and display this existing name map. Manual corrections remain authoritative. Do not infer a confident person or self identity from the unavailable bucket.

The existing additive reporting v1 vocabulary now has an Apple `speaker_separation_state` / `speaker_separation_failed` producer with fixed engine/reason fields. It requires managed mode and Share diagnostics and suppresses private-recording metadata before admission. This does not establish Linux producer capability, change Linux recovery classification, or add hardware telemetry. The [reporting handoff](recording-reporting-v1.md) remains canonical for shared fields and boundaries.

No Linux version, package, provider request, push or release accompanies this tracking commit.
