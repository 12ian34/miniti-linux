# Recording reporting v1, cross-client tracking

2026-11-03. Apple 3.7.0 and the shared backend implement this contract locally. Nothing is deployed or released. Linux has no new local transcription engine or reporting producer from this work.

The canonical contract is [Apple recording reporting v1](../../miniti/docs/reference/recording-reporting-contract.md), with fixed diagnostic vocabulary in `../miniti/test-contracts/v1/reporting-v1.json` and backend validation in `../miniti-api/lib/reportingContract.ts`.

## Usage

`POST /api/recording-usage` accepts only signed, enrolled installation requests with six body fields: `schema_version: 1`, UUID `report_id`, opaque case-sensitive `account_id`, `engine: "on_device"`, UTC `day`, and integer `duration_ms` from 1 through 14,400,000. Account IDs are `acct_…` tokens, not UUIDs; preserve their case. Installation IDs alone canonicalize to lowercase.

Reports are immutable and retain their original owner. The backend atomically compares receipts and increments separate local counters, with a 30-day delivery window, 35-day receipts, 400-day daily history and a 24-hour installation/day cap. Replay with the same payload is idempotent; changed reuse conflicts. The endpoint never charges Deepgram allowances or changes legacy cost inputs.

Linux continues its existing Deepgram session reporting. Do not send a duplicate report to this local-only endpoint. BYOK never creates automatic usage reports or enrolls for reporting. A future Linux local producer must account for managed local/private capture independently of Share diagnostics, use durable bounded checkpoints and queues, split UTC days, and enforce ownership before sends and after responses. No such producer is selected here.

Admin data adds separate Deepgram/on-device/total daily minutes and explicitly incomplete coverage. Uppercase/lowercase installation contributions are summed before rounding, and active installations are deduplicated. This does not prove coverage of older clients, offline or unenrolled installations, or interrupted capture.

## Optional diagnostics

The shared backend additively accepts fixed engine, lifecycle/transcription outcome, AI stage/mode, request UUID, coarse hardware and bounded lag/preparation fields through automatic event ingest and explicit Help sanitization. Missing fields remain unknown. Acceptance does not establish a Linux producer or recovery capability.

Keep managed-mode and Share diagnostics gates. No audio, transcript, prompt, arbitrary exception text, private-mode flag, meeting identity, serial number or hostname belongs in these fields. New per-recording speaker/lag metadata is suppressed in private sessions. Hardware snapshots, if later implemented, are installation-level and consent-gated. Usage reporting and diagnostic consent stay separate.

## Remaining Linux work

This commit tracks the additive shared contract only. Implement and test Linux producers in a separately scoped change if selected; preserve the existing unconfirmed recovery classification until its own producer evidence exists. Linux retains its separate version and release process. No push, package, production call or release is part of this handoff.
