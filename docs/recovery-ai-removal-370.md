# Apple 3.7.0 recovery, AI and removal handoff

Local tracking, 2026-11-03. No Linux producer or release is implied. The backend remains compatible with existing Linux requests.

- Additive `recovery-v1.json` defines planned restart, connection-restored and transcription-resumed milestones, fixed reasons, bounded connection/recovery generations and send/ingest/drop counters. These info events do not change incident policy. Future Linux producers must distinguish connection/capture from new words and keep generation ownership, consent/private gates and content exclusion. No audio replay or extra retry owner is authorized by this handoff.
- `/api/insights` accepts optional UUID-only `X-Client-Request-ID` for existing backend telemetry. Apple uses it for bounded live/final/saved/helper failure diagnostics; response shapes, routing and retry limits are unchanged. Linux may omit it. Absent correlation does not prove provider success or client failure cause.
- Local-duration ingestion now checks active account ownership/revocation atomically, including replay acknowledgements. Revocation removes local installation breakdowns; account deletion removes its aggregate. Global totals and bounded 35-day receipts remain, and existing billing/activity policy is unchanged. Linux has no local-duration producer in this scope; do not send a second report for Deepgram.

See the Apple [contract](../../miniti/docs/reference/recording-reporting-contract.md) and [release checks](../../miniti/docs/releases/3.7.0.md#combined-local-validation-2026-11-03). Current full Rust suite passes 220 tests with one optional live test ignored; `npm run build` passes on macOS. Native Linux audio validation and its independent release remain separate. Nothing is pushed.

Apple-only compatibility follow-through: prerecorded Watch diarization now applies per-chunk backpressure and speaker-session ownership fences. It does not change backend Watch usage or Linux audio contracts.
