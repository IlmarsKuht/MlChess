$ErrorActionPreference = "Stop"

Write-Host "Running focused human-timeout regression coverage..."
cargo test -p arena-server orchestration::tests::human_move_times_out_when_elapsed_exceeds_remaining_clock -- --nocapture
cargo test -p arena-server orchestration::tests::human_owner_times_out_without_submitted_move -- --nocapture
