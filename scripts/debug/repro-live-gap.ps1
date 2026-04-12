$ErrorActionPreference = "Stop"

Write-Host "Running focused live-gap regression coverage..."
cargo test -p arena-server api::tests::initial_stream_events_falls_back_to_snapshot_after_gap -- --nocapture
cargo test -p arena-server live::tests::replay_since_requests_snapshot_when_gap_falls_outside_buffer -- --nocapture
