param([int]$Runs=20)
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $taskRoot 'dist/p0-portable/media-browser.exe'
$rows=@()
for($i=0;$i -lt $Runs;$i++) {
    $timer=[Diagnostics.Stopwatch]::StartNew()
    $process=Start-Process -FilePath $exe -ArgumentList '--p0','--quit-after','1500' -WorkingDirectory $taskRoot -PassThru
    $idle=$process.WaitForInputIdle(10000)
    $idleMs=$timer.Elapsed.TotalMilliseconds
    $process.WaitForExit()
    $first=Get-Content -LiteralPath "$taskRoot/artifacts/p0-first-ui.json" -Raw | ConvertFrom-Json
    $rows+=[pscustomobject]@{run=$i+1;input_idle=$idle;external_input_idle_ms=[Math]::Round($idleMs,2);internal_first_ui_ms=$first.first_ui_ms;exit_code=$process.ExitCode}
}
$rows | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p0-startup-runs.json" -Encoding utf8
$values=@($rows.external_input_idle_ms | Sort-Object)
[pscustomobject]@{runs=$Runs;exe_sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash;external_idle_median_ms=$values[[int][Math]::Floor(($Runs-1)/2)];external_idle_p95_ms=$values[[int][Math]::Ceiling($Runs*.95)-1];scope='WaitForInputIdle is responsiveness evidence, not external visible-frame proof'} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p0-startup-summary.json" -Encoding utf8
Get-Content "$taskRoot/artifacts/p0-startup-summary.json"
