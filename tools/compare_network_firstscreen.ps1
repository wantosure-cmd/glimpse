param([string]$Output='target/v046-network-paired',[switch]$Resume)
$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskBase=[IO.Path]::GetFullPath((Join-Path $taskRoot $Output))
if(-not $taskBase.StartsWith($taskRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Output must remain in workspace'}
if(Test-Path -LiteralPath $taskBase){if(-not $Resume){throw 'Use a fresh paired output directory'}}
else {if($Resume){throw 'Resume directory does not exist'}; New-Item -ItemType Directory -Path $taskBase | Out-Null}
$taskLibrary='\\192.168.31.1\XiaoMi-NVMe(b520)\XiaomiCamera_00_E0A25A2763A2'
$taskVersions=@{
 old='dist/0.4.5-local-test-20261022-anchor-install/media-browser-portable/media-browser.exe'
 new='target/release/media-browser.exe'
}
$taskOrder=@('old','new','new','old','old','new')
$taskRows=@()
for($taskIndex=0;$taskIndex -lt $taskOrder.Count;$taskIndex++){
 $taskLabel=$taskOrder[$taskIndex]
 $taskRun=Join-Path $taskBase ('{0:D2}-{1}' -f $taskIndex,$taskLabel)
 # One process at a time, no instance handoff or user instance contacted.
 if(-not (Test-Path -LiteralPath $taskRun)){
  & "$PSScriptRoot/measure_user_load.ps1" -Directory $taskLibrary -ExecutablePath (Join-Path $taskRoot $taskVersions[$taskLabel]) -OutputRoot $taskRun -Seconds 12
 }
 $taskResult=Get-Content -LiteralPath (Join-Path $taskRun 'result.json') -Raw -Encoding utf8 | ConvertFrom-Json
 if(-not $taskResult.completed -or $taskResult.exit_code -ne 0){throw "Run failed at $taskRun; inspect existing handle and do not restart"}
 if(Get-Process -Id $taskResult.pid -ErrorAction SilentlyContinue){throw 'Prior diagnostic PID remains live; do not start a duplicate'}
 $taskMetrics=Get-Content -LiteralPath (Join-Path $taskRun 'draw-metrics.json') -Raw -Encoding utf8 | ConvertFrom-Json
 $taskFull=@($taskMetrics.rows | Where-Object {$_.visible_cards -gt 0 -and $_.visible_ready -eq $_.visible_cards -and $_.visible_pending_geometry -eq 0} | Select-Object -First 1)
 $taskFinal=@($taskMetrics.rows)[-1]
 $taskRows+=@{label=$taskLabel;result=$taskResult;first_all_visible=$(if($taskFull.Count){$taskFull[0]}else{$null});last=$taskFinal;raw=$taskRun}
 $taskRows | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $taskBase 'runs.json') -Encoding utf8
 Write-Output ("Run {0} {1}: first {2}ms, all visible {3}ms, final media {4}" -f $taskIndex,$taskLabel,$taskResult.first_native_preview_ms,$taskFull[0].elapsed_ms,$taskFinal.media)
}
