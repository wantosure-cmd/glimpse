param([int]$Runs=20,[string]$Output='artifacts/p5-startup.json',[string]$ExecutablePath='dist/browser-preview/media-browser.exe')
$ErrorActionPreference='Stop'
if($Runs -lt 3 -or $Runs -gt 100){throw 'Runs must be between 3 and 100'}
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $ExecutablePath)).Path
$taskBase=Join-Path $taskRoot ('target/startup-p5-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskBase | Out-Null
$taskRows=@()
for($taskI=0;$taskI -lt $Runs;$taskI++){
    $taskRunDirectory=Join-Path $taskBase $taskI
    New-Item -ItemType Directory -Path $taskRunDirectory | Out-Null
    $taskStart=[DateTime]::UtcNow
    $taskFirstPath=Join-Path $taskRunDirectory 'artifacts/browser-first-ui.json'
    $taskClock=[Diagnostics.Stopwatch]::StartNew()
    $taskProcess=Start-Process -FilePath $taskExe -ArgumentList '--instance-scope',('startup-'+$taskI+'-'+[DateTime]::Now.Ticks),'--data-dir',('"'+(Join-Path $taskBase $taskI)+'"'),'--first-ui-report',('"'+$taskFirstPath+'"'),'--quit-after','900' -WorkingDirectory $taskRunDirectory -PassThru
    $taskIdle=$taskProcess.WaitForInputIdle(10000)
    $taskIdleMs=$taskClock.Elapsed.TotalMilliseconds
    if(-not $taskIdle){throw "Application did not reach input idle: $($taskProcess.Id)"}
    $taskLoadedCrt=@($taskProcess.Modules | Where-Object ModuleName -ieq 'vcruntime140.dll' | ForEach-Object FileName)
    if($taskLoadedCrt.Count -ne 1 -or $taskLoadedCrt[0] -ne (Join-Path (Split-Path $taskExe) 'vcruntime140.dll')){throw 'Application did not load the packaged startup CRT'}
    if(-not $taskProcess.WaitForExit(15000)){throw "Diagnostic process still running: $($taskProcess.Id)"}
    $taskProcess.WaitForExit()
    if($taskProcess.ExitCode -ne 0){throw 'Startup process failed'}
    if((Get-Item -LiteralPath $taskFirstPath).LastWriteTimeUtc -lt $taskStart){throw 'Stale startup evidence'}
    $taskFirst=Get-Content -LiteralPath $taskFirstPath -Raw | ConvertFrom-Json
    $taskRows+=[pscustomobject]@{run=$taskI+1;pid=$taskProcess.Id;started_utc=$taskStart;input_idle=$taskIdle;external_input_idle_ms=$taskIdleMs;internal_first_ui_ms=$taskFirst.internal_first_ui_ms;loaded_crt=$taskLoadedCrt[0];exit_code=$taskProcess.ExitCode;first_ui_evidence=$taskFirstPath;working_directory=$taskRunDirectory}
}
$taskExternal=@($taskRows.external_input_idle_ms | Sort-Object)
$taskInternal=@($taskRows.internal_first_ui_ms | Sort-Object)
function Get-Median($Values) {
    $count=$Values.Count
    if($count % 2 -eq 0){return ($Values[$count/2-1]+$Values[$count/2])/2.0}
    return $Values[[int][Math]::Floor($count/2)]
}
$taskResult=[pscustomobject]@{
    scope='Release empty browser with fresh settings and isolated working directory per launch; warm OS file cache; WaitForInputIdle and internal first UI callback, not physical visible-frame or first-media presentation measurement'
    exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash
    exe_path=$taskExe
    runs=$Runs;fixture=$taskBase
    external_input_idle_median_ms=(Get-Median $taskExternal)
    external_input_idle_p95_ms=$taskExternal[[int][Math]::Ceiling($Runs*.95)-1]
    internal_first_ui_median_ms=(Get-Median $taskInternal)
    internal_first_ui_p95_ms=$taskInternal[[int][Math]::Ceiling($Runs*.95)-1]
    rows=$taskRows
}
$taskResult | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskRoot $Output) -Encoding utf8
$taskResult | Select-Object scope,exe_sha256,runs,external_input_idle_median_ms,external_input_idle_p95_ms,internal_first_ui_median_ms,internal_first_ui_p95_ms | ConvertTo-Json
