param(
    [string]$Directory='dist/0.3.0-local-test-20261021-slideshow',
    [string]$Prefix='v030',
    [int]$Seconds=120
)
$ErrorActionPreference='Stop'
if($Prefix -notmatch '^[a-zA-Z0-9_-]+$'){throw 'Invalid evidence prefix'}
if($Seconds -lt 60){throw 'Use at least 60 seconds for each scroll run'}
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskExeRelative=Join-Path $Directory 'media-browser-portable/media-browser.exe'
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $taskExeRelative)).Path
$taskData='target/'+$Prefix+'-performance-'+[Guid]::NewGuid().ToString('N')
$taskScan='target/'+$Prefix+'-scan-'+[Guid]::NewGuid().ToString('N')
New-Item -ItemType Directory -Path (Join-Path $taskRoot $taskScan) | Out-Null
& "$PSScriptRoot/collect_performance_machine.ps1" -Prefix $Prefix
python -X utf8 "$PSScriptRoot/verify_perf_fixture.py"
if($LASTEXITCODE -ne 0){throw 'Invalid generated fixture'}
Copy-Item -LiteralPath "$taskRoot/artifacts/p2-perf-fixture-verification.json" -Destination "$taskRoot/artifacts/$Prefix-perf-fixture-before.json"
Write-Output 'Enumerating 5000 generated media files'
$taskApp=Start-Process -FilePath $taskExe -ArgumentList '--scan-report','target/perf-media-5000','--db',($taskScan+'/library.db') -WorkingDirectory $taskRoot -WindowStyle Hidden -RedirectStandardOutput "$taskRoot/artifacts/$Prefix-scan.json" -RedirectStandardError "$taskRoot/artifacts/$Prefix-scan-stderr.log" -PassThru
$taskHandle=$taskApp.Handle
if(-not $taskApp.WaitForExit(90000)){throw "Scan process still live: PID $($taskApp.Id); inspect this process before restarting"}
$taskApp.WaitForExit()
if($taskApp.ExitCode -ne 0){throw "Enumeration failed: $($taskApp.ExitCode)"}
Write-Output 'Measuring 20 warm-OS-cache startup proxies'
& "$PSScriptRoot/measure_local_startup.ps1" -Runs 20 -Output "artifacts/$Prefix-startup20.json" -ExecutablePath $taskExeRelative
Write-Output 'Scrolling with a new application cache'
& "$PSScriptRoot/measure_browser_scroll.ps1" -Label "$Prefix-cold4k" -DataDir $taskData -Seconds $Seconds -ExecutablePath $taskExeRelative -MachineEvidence "artifacts/$Prefix-performance-machine.json"
python -X utf8 "$PSScriptRoot/summarize_browser_scroll.py" "$Prefix-cold4k"
if($LASTEXITCODE -ne 0){throw 'Cold application-cache scroll observations invalid'}
Write-Output 'Scrolling through the same application cache again'
& "$PSScriptRoot/measure_browser_scroll.ps1" -Label "$Prefix-revisit4k" -DataDir $taskData -Seconds $Seconds -ExecutablePath $taskExeRelative -MachineEvidence "artifacts/$Prefix-performance-machine.json"
python -X utf8 "$PSScriptRoot/summarize_browser_scroll.py" "$Prefix-revisit4k"
if($LASTEXITCODE -ne 0){throw 'Revisit scroll observations invalid'}
Write-Output 'Sampling idle CPU and working set for 30 seconds after warmup'
& "$PSScriptRoot/measure_browser_idle.ps1" -DataDir $taskData -Output "artifacts/$Prefix-idle30.json" -ExecutablePath $taskExeRelative
python -X utf8 "$PSScriptRoot/verify_perf_fixture.py"
if($LASTEXITCODE -ne 0){throw 'Post-run source verification failed'}
Copy-Item -LiteralPath "$taskRoot/artifacts/p2-perf-fixture-verification.json" -Destination "$taskRoot/artifacts/$Prefix-perf-fixture-after.json"
