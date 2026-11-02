[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [int]$Iterations = 3,
    [int]$SampleSeconds = 5
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$resultsDir = Join-Path $root 'audit\benchmarks\results'
New-Item -ItemType Directory -Force -Path $resultsDir | Out-Null
$ffmpegCommand = Get-Command ffmpeg -ErrorAction SilentlyContinue
$ffmpegVersion = 'not-found'
if ($null -ne $ffmpegCommand) {
    $ffmpegVersion = (& $ffmpegCommand.Source -version 2>$null | Select-Object -First 1)
}

function Get-ProcessSnapshot([int]$ProcessId) {
    $process = Get-Process -Id $ProcessId -ErrorAction Stop
    [pscustomobject]@{
        timestamp = (Get-Date).ToUniversalTime().ToString('o')
        workingSetBytes = $process.WorkingSet64
        privateBytes = $process.PrivateMemorySize64
        cpuSeconds = $process.TotalProcessorTime.TotalSeconds
        processCount = @(Get-Process -Name $process.ProcessName -ErrorAction SilentlyContinue).Count
    }
}

if (-not (Test-Path -LiteralPath $Executable)) {
    throw "Executable not found: $Executable"
}

$environment = [ordered]@{
    timestampUtc = (Get-Date).ToUniversalTime().ToString('o')
    computer = $env:COMPUTERNAME
    os = (Get-CimInstance Win32_OperatingSystem).Caption
    osVersion = (Get-CimInstance Win32_OperatingSystem).Version
    cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name)
    memoryBytes = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
    gpu = @((Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name))
    ffmpeg = $ffmpegVersion
    executable = (Resolve-Path $Executable).Path
}

$runs = @()
for ($iteration = 1; $iteration -le [Math]::Max(1, $Iterations); $iteration++) {
    $started = Get-Date
    $process = Start-Process -FilePath $Executable -PassThru
    $startup = ((Get-Date) - $started).TotalMilliseconds
    Start-Sleep -Milliseconds 1200
    $samples = @()
    for ($second = 0; $second -lt [Math]::Max(1, $SampleSeconds); $second++) {
        try { $samples += Get-ProcessSnapshot $process.Id } catch { break }
        Start-Sleep -Seconds 1
    }
    $stopStarted = Get-Date
    $forcedStop = $false
    if (-not $process.HasExited) { $process.CloseMainWindow() | Out-Null }
    if (-not $process.WaitForExit(5000)) {
        & taskkill.exe /PID $process.Id /T /F 2>$null | Out-Null
        $forcedStop = $true
        $process.WaitForExit()
    }
    $stopMs = ((Get-Date) - $stopStarted).TotalMilliseconds
    $runs += [pscustomobject]@{
        iteration = $iteration
        startupMs = [Math]::Round($startup, 2)
        stopMs = [Math]::Round($stopMs, 2)
        stopMode = if ($forcedStop) { 'forced-process-tree' } else { 'graceful-window-close' }
        samples = $samples
        capture = 'not-run'
        clipSaveLatencyMs = 'not-run'
        observedFps = 'not-run'
        frameDrops = 'not-run'
    }
}

$report = [ordered]@{ environment = $environment; runs = $runs }
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$jsonPath = Join-Path $resultsDir "windows-$stamp.json"
$csvPath = Join-Path $resultsDir "windows-$stamp.csv"
$report | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $jsonPath
$runs | ForEach-Object {
    [pscustomobject]@{
        iteration = $_.iteration
        startupMs = $_.startupMs
        stopMs = $_.stopMs
        peakWorkingSetBytes = @($_.samples.workingSetBytes | Measure-Object -Maximum).Maximum
        peakPrivateBytes = @($_.samples.privateBytes | Measure-Object -Maximum).Maximum
        processCountMax = @($_.samples.processCount | Measure-Object -Maximum).Maximum
        capture = $_.capture
        clipSaveLatencyMs = $_.clipSaveLatencyMs
        observedFps = $_.observedFps
        frameDrops = $_.frameDrops
    }
} | Export-Csv -NoTypeInformation -Encoding UTF8 $csvPath

Write-Host "JSON: $jsonPath"
Write-Host "CSV:  $csvPath"
