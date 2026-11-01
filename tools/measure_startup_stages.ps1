param([int]$Runs=20,[string]$ExecutablePath='target/release/media-browser.exe',[string]$Output='artifacts/v047-startup-stages.json',[int]$QuitAfterMs=2000,[switch]$InspectModules,[switch]$Prelude,[switch]$NoRedirect)
$ErrorActionPreference='Stop'
if($Runs -lt 3 -or $Runs -gt 30){throw 'Use 3..30 diagnostic runs'}
if($QuitAfterMs -lt 900 -or $QuitAfterMs -gt 5000){throw 'Lifetime must be 900..5000ms'}
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $ExecutablePath)).Path
$taskOutput=[IO.Path]::GetFullPath((Join-Path $taskRoot $Output))
if(-not $taskOutput.StartsWith($taskRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Report must remain inside workspace'}
if(Test-Path -LiteralPath $taskOutput){throw 'Do not overwrite previous diagnostic observations'}
$taskBase=Join-Path $taskRoot ('target/startup-stages-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskBase | Out-Null
$taskReport=[ordered]@{exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash;executable=$taskExe;fixture=$taskBase;finished=$false;full_acceptance_pass=$false;runs_requested=$Runs;quit_after_ms=$QuitAfterMs;inspect_modules=[bool]$InspectModules;stdout_stderr_redirected=(-not [bool]$NoRedirect);rows=@();scope='Sequential empty browser launches, fresh settings each time, warm OS cache not flushed. Stage timing since main and GL callback do not prove physical presentation or interactivity. No media library or user instance contacted.'}
if($Prelude){
 $taskPrefix=[IO.Path]::GetFileNameWithoutExtension($taskOutput)+'-prelude'
 & "$PSScriptRoot/collect_performance_machine.ps1" -Prefix $taskPrefix
 python -X utf8 "$PSScriptRoot/verify_perf_fixture.py"
 if($LASTEXITCODE -ne 0){throw 'Generated prelude fixture hashes failed'}
 $taskScanOut=Join-Path $taskBase 'prelude-scan.json'
 $taskScan=Start-Process -FilePath $taskExe -ArgumentList '--scan-report',('"'+(Join-Path $taskRoot 'target/perf-media-5000')+'"'),'--db',('"'+(Join-Path $taskBase 'prelude-index.sqlite')+'"') -WorkingDirectory $taskRoot -WindowStyle Hidden -RedirectStandardOutput $taskScanOut -RedirectStandardError (Join-Path $taskBase 'prelude-scan-stderr.log') -PassThru
 $taskScanHandle=$taskScan.Handle
 if(-not $taskScan.WaitForExit(65000)){throw "Own prelude scan remains live PID $($taskScan.Id); inspect, do not restart"}
 $taskScan.WaitForExit()
 if($taskScan.ExitCode -ne 0){throw 'Prelude scan failed; no startup comparison claimed'}
 $taskScanResult=Get-Content -LiteralPath $taskScanOut -Raw -Encoding utf8 | ConvertFrom-Json
 if($taskScanResult.media_count -ne 5000 -or $taskScanResult.errors.Count -ne 0){throw 'Invalid prelude scan'}
 $taskReport.prelude=[ordered]@{scan=$taskScanResult;scan_file=$taskScanOut;scan_pid=$taskScan.Id;scan_exit_code=$taskScan.ExitCode;machine_prefix=$taskPrefix}
}
for($taskIndex=0;$taskIndex -lt $Runs;$taskIndex++){
 $taskWork=Join-Path $taskBase ([string]$taskIndex)
 New-Item -ItemType Directory -Path $taskWork | Out-Null
 $taskTrace=Join-Path $taskWork 'startup-trace.json'
 $taskFirstPath=Join-Path $taskWork 'artifacts/browser-first-ui.json'
 $taskClock=[Diagnostics.Stopwatch]::StartNew()
 $taskLaunch=@{FilePath=$taskExe;ArgumentList=@('--instance-scope',('startup-stage-'+[guid]::NewGuid().ToString('N')),'--data-dir',('"'+(Join-Path $taskWork 'data')+'"'),'--startup-trace',('"'+$taskTrace+'"'),'--first-ui-report',('"'+$taskFirstPath+'"'),'--quit-after',([string]$QuitAfterMs));WorkingDirectory=$taskWork;PassThru=$true}
 if(-not $NoRedirect){$taskLaunch.RedirectStandardOutput=Join-Path $taskWork 'stdout.log';$taskLaunch.RedirectStandardError=Join-Path $taskWork 'stderr.log'}
 $taskProc=Start-Process @taskLaunch
 $taskHandle=$taskProc.Handle
 $taskIdle=$taskProc.WaitForInputIdle(10000)
 $taskIdleMs=$taskClock.Elapsed.TotalMilliseconds
 $taskModuleMs=$null
 if($InspectModules){
  $taskModuleClock=[Diagnostics.Stopwatch]::StartNew()
  $taskModules=@($taskProc.Modules | Select-Object -ExpandProperty FileName)
  $taskModuleMs=$taskModuleClock.Elapsed.TotalMilliseconds
  if(-not $taskModules.Count){throw 'No module snapshot returned'}
 }
 if(-not $taskProc.WaitForExit(20000)){throw "Own startup process still live PID $($taskProc.Id); inspect it, do not restart"}
 $taskProc.WaitForExit()
 if($taskProc.ExitCode -ne 0){throw "Startup diagnostic failed PID $($taskProc.Id) exit $($taskProc.ExitCode)"}
 $taskStages=Get-Content -LiteralPath $taskTrace -Raw -Encoding utf8 | ConvertFrom-Json
 if($taskStages.pid -ne $taskProc.Id){throw 'Trace does not belong to this process'}
 $taskNames=@($taskStages.events.stage)
 foreach($taskRequired in @('main_enter','before_run_native','creation_context_ready','app_creator_done','first_ui_entry','first_ui_complete','native_loop_returned')){
  if($taskNames -notcontains $taskRequired){throw "Missing diagnostic stage $taskRequired"}
 }
 $taskFirst=Get-Content -LiteralPath $taskFirstPath -Raw -Encoding utf8 | ConvertFrom-Json
 $taskReport.rows+=@{run=($taskIndex+1);pid=$taskProc.Id;exit_code=$taskProc.ExitCode;input_idle=$taskIdle;external_input_idle_ms=$taskIdleMs;module_query_ms=$taskModuleMs;internal_first_ui_ms=$taskFirst.internal_first_ui_ms;trace_file=$taskTrace;trace=$taskStages}
 $taskReport | ConvertTo-Json -Depth 9 | Set-Content -LiteralPath $taskOutput -Encoding utf8
 Write-Output ("Run {0}: internal first UI {1}ms" -f ($taskIndex+1),$taskFirst.internal_first_ui_ms)
}
$taskReport.finished=$true
$taskReport | ConvertTo-Json -Depth 9 | Set-Content -LiteralPath $taskOutput -Encoding utf8
Write-Output "Saved $taskOutput"
