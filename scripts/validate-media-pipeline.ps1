$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$outputDir = Join-Path $root 'target\validation'
$output = Join-Path $outputDir 'courage-demo.mp4'
$trimmed = Join-Path $outputDir 'courage-demo-trimmed.mp4'

New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue $output, $trimmed, ($output -replace '\.mp4$', '.sqlite')

cargo run --quiet -p courage-cli -- demo `
  --output $output `
  --seconds 4 `
  --replay-seconds 4

if (-not (Test-Path $output)) { throw "Expected output was not created: $output" }

$videoCodec = (ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 $output).Trim()
$audioCodec = (ffprobe -v error -select_streams a:0 -show_entries stream=codec_name -of csv=p=0 $output).Trim()
$duration = [double](ffprobe -v error -show_entries format=duration -of csv=p=0 $output).Trim()

if ($videoCodec -ne 'h264') { throw "Expected h264 video, got $videoCodec" }
if ($audioCodec -ne 'aac') { throw "Expected aac audio, got $audioCodec" }
if ($duration -le 3.5) { throw "Expected duration > 3.5 seconds, got $duration" }

cargo run --quiet -p courage-cli -- trim `
  --input $output `
  --output $trimmed `
  --start 1 `
  --end 3

$trimmedDuration = [double](ffprobe -v error -show_entries format=duration -of csv=p=0 $trimmed).Trim()
if ($trimmedDuration -le 1.5 -or $trimmedDuration -ge 2.5) {
  throw "Expected trimmed duration between 1.5 and 2.5 seconds, got $trimmedDuration"
}

Write-Host "validated video=$videoCodec audio=$audioCodec duration=$duration seconds trim_duration=$trimmedDuration seconds"
