param([string]$Directory='dist/0.1.0-local-test-20261020-folders',[string]$Prefix='folders')
if($Prefix -notmatch '^[a-zA-Z0-9_-]+$'){throw 'Invalid prefix'}
# Current immutable delivery: retain failed observations, no WMI dependency.
$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskRelative=Join-Path $Directory 'media-browser-portable/media-browser.exe'
$taskExe=Join-Path $taskRoot $taskRelative
$taskData="target/$Prefix-performance-"+[guid]::NewGuid().ToString('N')
$taskScan="target/$Prefix-scan-"+[guid]::NewGuid().ToString('N')
New-Item -ItemType Directory -Path (Join-Path $taskRoot $taskScan) | Out-Null
$taskApp=Start-Process -FilePath $taskExe -ArgumentList '--scan-report','target/perf-media-5000','--db',($taskScan+'/library.db') -WorkingDirectory $taskRoot -WindowStyle Hidden -RedirectStandardOutput "$taskRoot/artifacts/$Prefix-scan.json" -RedirectStandardError "$taskRoot/artifacts/$Prefix-scan-stderr.log" -PassThru
if(-not $taskApp.WaitForExit(90000)){throw "Scan live PID $($taskApp.Id); inspect, do not restart"};$taskApp.WaitForExit()
if($taskApp.ExitCode -ne 0){throw 'Scan failed'}
$taskProbe=Start-Process -FilePath $taskExe -ArgumentList '--instance-scope',('folders-normal-'+[guid]::NewGuid().ToString('N')),'--data-dir',("target/$Prefix-normal-"+[guid]::NewGuid().ToString('N')),'--quit-after','1500' -WorkingDirectory $taskRoot -RedirectStandardOutput "$taskRoot/artifacts/$Prefix-normal-start-stdout.log" -RedirectStandardError "$taskRoot/artifacts/$Prefix-normal-start-stderr.log" -PassThru
if(-not $taskProbe.WaitForExit(15000)){throw "Normal probe live PID $($taskProbe.Id); inspect, do not restart"};$taskProbe.WaitForExit()
$taskProbeOk=$taskProbe.ExitCode -eq 0
[pscustomobject]@{exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash;pid=$taskProbe.Id;exit_code=$taskProbe.ExitCode;normal_start_pass=$taskProbeOk;stderr=(Get-Content "$taskRoot/artifacts/$Prefix-normal-start-stderr.log" -Raw);scope='One isolated normal app launch using its supported instance scope; existing user app not stopped'} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/$Prefix-normal-start.json" -Encoding utf8
if($taskProbeOk){
 try{& "$PSScriptRoot/measure_local_startup.ps1" -Runs 20 -Output "artifacts/$Prefix-startup.json" -ExecutablePath $taskRelative}catch{[pscustomobject]@{verified=$false;error=$_.Exception.Message} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/$Prefix-startup-blocked.json" -Encoding utf8}
}else{[pscustomobject]@{verified=$false;requested_runs=20;completed_measurement_runs=0;reason='Normal instance startup rejected in current restricted environment; no benchmark-mode substitute used'} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/$Prefix-startup-blocked.json" -Encoding utf8}
& "$PSScriptRoot/measure_browser_scroll.ps1" -Label "$Prefix-final-cold4k" -DataDir $taskData -Seconds 120 -ExecutablePath $taskRelative -MachineEvidence "artifacts/$Prefix-performance-machine.json"
python -X utf8 "$PSScriptRoot/summarize_browser_scroll.py" "$Prefix-final-cold4k"
if($LASTEXITCODE -ne 0){throw 'Cold observations failed'}
& "$PSScriptRoot/measure_browser_scroll.ps1" -Label "$Prefix-final-revisit4k" -DataDir $taskData -Seconds 120 -ExecutablePath $taskRelative -MachineEvidence "artifacts/$Prefix-performance-machine.json"
python -X utf8 "$PSScriptRoot/summarize_browser_scroll.py" "$Prefix-final-revisit4k"
if($LASTEXITCODE -ne 0){throw 'Revisit observations failed'}
if($taskProbeOk){
 try{& "$PSScriptRoot/measure_browser_idle.ps1" -DataDir $taskData -Output "artifacts/$Prefix-browser-idle.json" -ExecutablePath $taskRelative}catch{[pscustomobject]@{verified=$false;error=$_.Exception.Message} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/$Prefix-idle-blocked.json" -Encoding utf8}
}else{[pscustomobject]@{verified=$false;reason='Normal app instance could not start; idle CPU not inferred from benchmark continuous repaint'} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/$Prefix-idle-blocked.json" -Encoding utf8}
python -X utf8 "$PSScriptRoot/verify_perf_fixture.py"
if($LASTEXITCODE -ne 0){throw 'Post-run sources changed'}
Copy-Item -LiteralPath "$taskRoot/artifacts/p2-perf-fixture-verification.json" -Destination "$taskRoot/artifacts/$Prefix-perf-fixture-after.json"
