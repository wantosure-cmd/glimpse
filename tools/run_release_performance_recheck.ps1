param(
    [string]$Directory='dist/0.1.0-local-test-20261019-overlay-final',
    [string]$Prefix='overlay'
)
$ErrorActionPreference='Stop'
if($Prefix -notmatch '^[a-zA-Z0-9_-]+$'){throw 'Invalid evidence prefix'}
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
New-Item -ItemType Directory -Path (Join-Path $taskRoot 'artifacts') -Force | Out-Null
$taskExeRelative=Join-Path $Directory 'media-browser-portable/media-browser.exe'
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $taskExeRelative)).Path
$taskData='target/'+$Prefix+'-perf-'+[Guid]::NewGuid().ToString('N')
$taskScan='target/'+$Prefix+'-scan-'+[Guid]::NewGuid().ToString('N')
New-Item -ItemType Directory -Path (Join-Path $taskRoot $taskScan) | Out-Null
Write-Output 'Record current machine and fixture disk (read only)'
$taskCpu=Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors
$taskMemory=Get-CimInstance Win32_ComputerSystem | Select-Object TotalPhysicalMemory
$taskVideo=Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,CurrentHorizontalResolution,CurrentVerticalResolution
$taskOs=Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' | Select-Object ProductName,DisplayVersion,CurrentBuildNumber,UBR
$taskDisk=Get-Partition -DriveLetter (Get-Item -LiteralPath $taskRoot).PSDrive.Name | Get-Disk | Select-Object Number,FriendlyName,BusType,Size
[pscustomobject]@{recorded_utc=[datetime]::UtcNow;cpu=$taskCpu;memory=$taskMemory;video=$taskVideo;os=$taskOs;storage=$taskDisk} | ConvertTo-Json -Depth 6 | Set-Content "$taskRoot/artifacts/$Prefix-performance-machine.json" -Encoding utf8
Write-Output 'Check generated 5000-file fixture before measurements'
python -X utf8 "$PSScriptRoot/verify_perf_fixture.py"
if($LASTEXITCODE -ne 0){throw 'Fixture verification failed'}
Copy-Item -LiteralPath "$taskRoot/artifacts/p2-perf-fixture-verification.json" -Destination "$taskRoot/artifacts/$Prefix-perf-fixture-before.json"
Write-Output 'Measure enumeration/indexing separately from image decode'
$taskApp=Start-Process -FilePath $taskExe -ArgumentList '--scan-report','target/perf-media-5000','--db',($taskScan+'/library.db') -WorkingDirectory $taskRoot -WindowStyle Hidden -RedirectStandardOutput "$taskRoot/artifacts/$Prefix-scan.json" -RedirectStandardError "$taskRoot/artifacts/$Prefix-scan-stderr.log" -PassThru
if(-not $taskApp.WaitForExit(90000)){throw "Scan still running: PID $($taskApp.Id); inspect instead of restarting"}
$taskApp.WaitForExit()
if($taskApp.ExitCode -ne 0){throw 'Enumeration failed'}
Write-Output 'Measure 20 warm-OS-cache launches of delivered application'
& "$PSScriptRoot/measure_local_startup.ps1" -Runs 20 -Output "artifacts/$Prefix-startup.json" -ExecutablePath $taskExeRelative
Write-Output 'Measure 120 seconds scrolling with a fresh application cache'
& "$PSScriptRoot/measure_browser_scroll.ps1" -Label "$Prefix-final-cold4k" -DataDir $taskData -Seconds 120 -ExecutablePath $taskExeRelative -MachineEvidence "artifacts/$Prefix-performance-machine.json"
python -X utf8 "$PSScriptRoot/summarize_browser_scroll.py" "$Prefix-final-cold4k"
if($LASTEXITCODE -ne 0){throw 'Cold scroll observations invalid'}
Write-Output 'Measure 120 seconds revisiting the same application cache'
& "$PSScriptRoot/measure_browser_scroll.ps1" -Label "$Prefix-final-revisit4k" -DataDir $taskData -Seconds 120 -ExecutablePath $taskExeRelative -MachineEvidence "artifacts/$Prefix-performance-machine.json"
python -X utf8 "$PSScriptRoot/summarize_browser_scroll.py" "$Prefix-final-revisit4k"
if($LASTEXITCODE -ne 0){throw 'Revisit scroll observations invalid'}
Write-Output 'Measure 30 seconds idle CPU after 15 seconds warmup'
& "$PSScriptRoot/measure_browser_idle.ps1" -DataDir $taskData -Output "artifacts/$Prefix-browser-idle.json" -ExecutablePath $taskExeRelative
python -X utf8 "$PSScriptRoot/verify_perf_fixture.py"
if($LASTEXITCODE -ne 0){throw 'Post-run source hash verification failed'}
Copy-Item -LiteralPath "$taskRoot/artifacts/p2-perf-fixture-verification.json" -Destination "$taskRoot/artifacts/$Prefix-perf-fixture-after.json"
python -X utf8 "$PSScriptRoot/verify_release_recheck.py" $Directory --prefix $Prefix
if($LASTEXITCODE -ne 0){throw 'Current delivery evidence binding failed'}
