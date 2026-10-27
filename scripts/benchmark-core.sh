#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="$root_dir/target/benchmarks"
mkdir -p "$output_dir"

for run in 1 2 3; do
  output="$output_dir/run-${run}.mp4"
  start_ns="$(date +%s%N)"
  cargo run --quiet -p courage-cli -- demo \
    --output "$output" \
    --seconds 3 \
    --replay-seconds 3 \
    --no-audio
  end_ns="$(date +%s%N)"
  elapsed_ms="$(( (end_ns - start_ns) / 1000000 ))"
  printf 'run=%s wall_ms=%s output=%s\n' "$run" "$elapsed_ms" "$output"
done
