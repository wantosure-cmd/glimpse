param([string]$DataDir='target/perf-ui-4k-20261017',[string]$Output='artifacts/p2-browser-idle.json',[string]$ExecutablePath='dist/browser-preview/media-browser.exe')
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $ExecutablePath)).Path
$taskScope='idle-measurement-'+[Guid]::NewGuid().ToString('N')
$taskApp=Start-Process -FilePath $taskExe -ArgumentList '--instance-scope',$taskScope,'--open','target/perf-media-5000','--data-dir',('"'+$DataDir+'"'),'--quit-after','47000' -WorkingDirectory $taskRoot -PassThru
$taskProcessHandle=$taskApp.Handle # Preserve exit-code access under Windows PowerShell 5.1.
Start-Sleep -Seconds 15
if($taskApp.HasExited){throw 'Application exited before idle sampling'}
$taskApp.Refresh()
$taskCpuStart=$taskApp.TotalProcessorTime.TotalSeconds
$taskTimer=[Diagnostics.Stopwatch]::StartNew()
$taskRows=[Collections.Generic.List[object]]::new()
while($taskTimer.Elapsed.TotalSeconds -lt 30){
    $taskApp.Refresh()
    if($taskApp.HasExited){throw 'Application exited during idle sampling'}
    $taskRows.Add([pscustomobject]@{elapsed_ms=$taskTimer.Elapsed.TotalMilliseconds;cpu_seconds=$taskApp.TotalProcessorTime.TotalSeconds;working_set_bytes=$taskApp.WorkingSet64})
    Start-Sleep -Milliseconds 500
}
$taskApp.Refresh()
$taskCpuDelta=$taskApp.TotalProcessorTime.TotalSeconds-$taskCpuStart
$taskElapsed=$taskTimer.Elapsed.TotalSeconds
$taskApp.WaitForExit()
[pscustomobject]@{
    scope='Release normal browser, 5000 valid media, first-screen cache already populated, 15-second warmup then 30-second idle; no video playback or diagnostic continuous repaint'
    exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash
    exe_path=$taskExe
    cpu_seconds=$taskCpuDelta;elapsed_seconds=$taskElapsed;logical_processors=[Environment]::ProcessorCount
    normalized_total_cpu_percent=100*$taskCpuDelta/$taskElapsed/[Environment]::ProcessorCount
    data_dir=$DataDir;exit_code=$taskApp.ExitCode;samples=$taskRows
} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $taskRoot $Output) -Encoding utf8
Get-Content (Join-Path $taskRoot $Output) -TotalCount 12
