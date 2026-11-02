# Run in an interactive Windows desktop against a locally built/installed Courage.
# No name-based process termination. Results include exact owner/session/PID identity.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [ValidateRange(10,100)][int]$Cycles = 10,
    [string]$ResultsDir = (Join-Path ([IO.Path]::GetTempPath()) ('courage-lifecycle-' + [guid]::NewGuid())),
    [switch]$ConfirmMicrophoneRelease,
    [switch]$RunBackendTests
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'This test needs an interactive Windows desktop.' }
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Path $ResultsDir -Force | Out-Null
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$records = [Collections.Generic.List[object]]::new()
$owned = [Collections.Generic.List[object]]::new()
$app = $null
$stderr = Join-Path $ResultsDir 'courage.stderr.log'

function Wait-For([scriptblock]$Check, [string]$Description, [int]$Seconds = 30) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        $value = & $Check
        if ($value) { return $value }
        Start-Sleep -Milliseconds 100 # poll observed state, never assume cleanup from a delay
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out: $Description"
}

function Read-Events {
    if (Test-Path $stderr) {
        Get-Content $stderr | ForEach-Object {
            try { $_ | ConvertFrom-Json -ErrorAction Stop } catch { }
        }
    }
}

function Wait-Event([string]$Name, [string]$Session = '', [string[]]$Exclude = @()) {
    Wait-For {
        Read-Events | Where-Object {
            $_.event -eq $Name -and $_.ownerPid -eq $app.Id -and
            (!$Session -or $_.sessionId -eq $Session) -and $_.sessionId -notin $Exclude
        } | Select-Object -Last 1
    } "$Name / session=$Session / owner=$($app.Id)"
}

function Click-Button([string]$NamePattern) {
    $button = Wait-For {
        $app.Refresh()
        if ($app.HasExited) { throw 'Courage exited before the UI action' }
        if ($app.MainWindowHandle -eq [IntPtr]::Zero) { return $null }
        $root = [Windows.Automation.AutomationElement]::FromHandle($app.MainWindowHandle)
        $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button)
        $root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
            Where-Object { $_.Current.Name -match $NamePattern -and $_.Current.IsEnabled } | Select-Object -First 1
    } "accessible enabled button $NamePattern"
    $pattern = $button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
}

function Track-Recording($event) {
    $native = Get-CimInstance Win32_Process -Filter "ProcessId=$($event.pid)"
    if (!$native -or $native.ParentProcessId -ne $app.Id -or $native.CommandLine -notlike '*segment-%05d.mp4*') {
        throw "Ownership mismatch session=$($event.sessionId) pid=$($event.pid)"
    }
    $process = Get-Process -Id $event.pid
    $null = $process.Handle # keep this exact process object, resistant to PID reuse
    $item = [pscustomobject]@{ sessionId=$event.sessionId; pid=$event.pid; directory=$event.directory; process=$process }
    $owned.Add($item)
    Wait-For { @(Get-ChildItem -LiteralPath $item.directory -Filter '*.mp4' | Where-Object Length -gt 0).Count -gt 0 } "recording activity for $($item.sessionId)" | Out-Null
    return $item
}

function Verify-Released($item, [string]$Scenario) {
    if (!$item.process.WaitForExit(10000)) { throw "Owned PID still alive: $($item.pid) session=$($item.sessionId)" }
    if (Test-Path -LiteralPath $item.directory) { throw "Session still exists: $($item.directory) pid=$($item.pid)" }
    $microphone = 'NOT VERIFIED: privacy indicator requires observation'
    if ($ConfirmMicrophoneRelease) {
        $answer = Read-Host "Verify Windows microphone privacy indicator no longer shows this Courage FFmpeg after $Scenario. Type released, failed, or unavailable"
        if ($answer -eq 'failed') { throw "Microphone release failed: session=$($item.sessionId) pid=$($item.pid)" }
        if ($answer -eq 'released') { $microphone = 'PASS: operator observed privacy indicator' }
    }
    $records.Add([pscustomobject]@{ scenario=$Scenario; ownerPid=$app.Id; sessionId=$item.sessionId; ffmpegPid=$item.pid; processExited=$true; sessionRemoved=$true; microphone=$microphone })
}

try {
    if ($RunBackendTests) {
        cargo test -p courage-desktop --lib
        if ($LASTEXITCODE) { throw 'Backend ownership / Windows job tests failed' }
        cargo test -p courage-desktop real_ffmpeg -- --ignored --nocapture
        if ($LASTEXITCODE) { throw 'Real FFmpeg integration tests failed' }
    }
    $app = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $stderr -RedirectStandardOutput (Join-Path $ResultsDir 'courage.stdout.log')
    $null = $app.Handle
    $seen = @()
    foreach ($save in @($false, $true)) {
        for ($cycle = 1; $cycle -le $Cycles; $cycle++) {
            if ($seen.Count -gt 0) { Click-Button '^Start buffer$' }
            $event = Wait-Event 'capture.recording' '' $seen
            $seen += $event.sessionId
            $item = Track-Recording $event
            if ($save) {
                Wait-For { @(Get-ChildItem -LiteralPath $item.directory -Filter '*.mp4').Count -ge 3 } 'closed replay segments' | Out-Null
                $before = @(Read-Events | Where-Object event -eq 'library.clip_saved').Count
                Click-Button '^Save last '
                Wait-For { @(Read-Events | Where-Object event -eq 'library.clip_saved').Count -gt $before } 'saved replay' | Out-Null
            }
            Click-Button '^Stop buffer$'
            Wait-Event 'capture.stopped' $item.sessionId | Out-Null
            Verify-Released $item "cycle=$cycle save=$save"
        }
    }
    Click-Button '^Start buffer$'
    $item = Track-Recording (Wait-Event 'capture.recording' '' $seen)
    $app.Refresh()
    if (!$app.CloseMainWindow()) { throw 'Could not send main-window Close' }
    if (!$app.WaitForExit(30000)) { throw 'Close did not fully exit Courage' }
    Verify-Released $item 'main-window Close while recording'
    foreach ($item in $owned) { if (!$item.process.HasExited) { throw "Orphaned owned PID $($item.pid) session=$($item.sessionId)" } }
    $records | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $ResultsDir 'results.json')
    Write-Host "Process/cleanup checks passed. Microphone results remain separate. Evidence: $ResultsDir"
} catch {
    $records.Add([pscustomobject]@{ scenario='failure'; error=$_.Exception.Message; owned=@($owned | Select-Object sessionId,pid,directory) })
    $records | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $ResultsDir 'results.json')
    throw
} finally {
    if ($app -and !$app.HasExited) {
        $null = $app.CloseMainWindow()
        if (!$app.WaitForExit(30000)) {
            # Only the exact process object started by this script. Its jobs must
            # release children; keep any failure evidence rather than killing names.
            $app.Kill()
            $app.WaitForExit()
        }
    }
}
