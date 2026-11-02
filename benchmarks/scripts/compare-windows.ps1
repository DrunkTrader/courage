[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Baseline,
    [Parameter(Mandatory = $true)]
    [string]$PostFix,
    [string]$Output
)

$ErrorActionPreference = 'Stop'

function Read-Report([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { throw "Report not found: $Path" }
    Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
}

function Average([object[]]$Runs, [string]$Property) {
    $values = @($Runs | ForEach-Object { [double]($_.$Property) } | Where-Object { $_ -ge 0 })
    if ($values.Count -eq 0) { return $null }
    return [Math]::Round((($values | Measure-Object -Average).Average), 2)
}

$before = Read-Report $Baseline
$after = Read-Report $PostFix
$comparison = [pscustomobject]@{
    generatedUtc = (Get-Date).ToUniversalTime().ToString('o')
    baseline = (Resolve-Path $Baseline).Path
    postFix = (Resolve-Path $PostFix).Path
    startupMs = [pscustomobject]@{
        baseline = Average $before.runs 'startupMs'
        postFix = Average $after.runs 'startupMs'
    }
    stopMs = [pscustomobject]@{
        baseline = Average $before.runs 'stopMs'
        postFix = Average $after.runs 'stopMs'
    }
    peakWorkingSetBytes = [pscustomobject]@{
        baseline = Average @($before.runs | ForEach-Object { [pscustomobject]@{ value = @($_.samples.workingSetBytes | Measure-Object -Maximum).Maximum } }) 'value'
        postFix = Average @($after.runs | ForEach-Object { [pscustomobject]@{ value = @($_.samples.workingSetBytes | Measure-Object -Maximum).Maximum } }) 'value'
    }
    captureMetrics = 'Compare only values populated by an interactive capture run.'
}

if ($Output) {
    $comparison | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -LiteralPath $Output
    Write-Host "Comparison: $Output"
} else {
    $comparison | ConvertTo-Json -Depth 8
}
