#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output="$root_dir/target/validation/courage-demo.mp4"
rm -f "$output" "${output%.mp4}.sqlite"

cargo run --quiet -p courage-cli -- demo \
  --output "$output" \
  --seconds 4 \
  --replay-seconds 4

test -s "$output"
video_codec="$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name \
  -of csv=p=0 "$output")"
audio_codec="$(ffprobe -v error -select_streams a:0 -show_entries stream=codec_name \
  -of csv=p=0 "$output")"
duration="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$output")"

test "$video_codec" = "h264"
test "$audio_codec" = "aac"
awk -v duration="$duration" 'BEGIN { exit !(duration > 3.5) }'

trimmed="${output%.mp4}-trimmed.mp4"
cargo run --quiet -p courage-cli -- trim \
  --input "$output" \
  --output "$trimmed" \
  --start 1 \
  --end 3
test -s "$trimmed"
trimmed_duration="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$trimmed")"
awk -v duration="$trimmed_duration" 'BEGIN { exit !(duration > 1.5 && duration < 2.5) }'
printf 'validated video=%s audio=%s duration=%ss file=%s trim_duration=%ss trim_file=%s\n' \
  "$video_codec" "$audio_codec" "$duration" "$output" "$trimmed_duration" "$trimmed"
