# 2026-11-03 test-foundation handoff

Apple 3.7.0 [test foundation](../../miniti/docs/plans/test-foundation.md) exposed an incremental Questions prompt defect: the instruction referred to earlier questions but did not include their values. The backend and Linux BYOK prompt now include the existing context note. Apple BYOK already supplied its rolling state. Linux adds a regression asserting the actual prior question and transcript delta appear in the prompt.

Local validation: `cargo test --manifest-path src-tauri/Cargo.toml insights::provider::tests` passed all 5 selected tests on macOS. The full Rust suite subsequently passed 219 tests with 1 live-enrollment case ignored. This is Rust prompt validation, not Linux device/audio evidence. No release or push is authorized.

The canonical synthetic meeting/auth fixtures now live in the public Apple repo at `test-contracts/v1`, with SHA-256 pins and an import/check script. Apple/backend copies and a Swift → backend → Swift exchange run locally. Linux's existing golden assertions remain independent; adopting the canonical package in Linux CI is a tracked follow-up. No wire fields changed in this foundation slice.

Model retirement, usage and diagnostics retain the scope in `PLAN.md`. This prompt fix does not select new models, change account credentials or enable a producer.
