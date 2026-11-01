$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $taskRoot 'dist/p0-portable/media-browser.exe'
$p=Start-Process -FilePath $exe -ArgumentList '--p0','--quit-after','38000' -WorkingDirectory $taskRoot -PassThru
$null=$p.WaitForInputIdle(10000)
Start-Sleep -Seconds 3
$p.Refresh();$cpuStart=$p.TotalProcessorTime.TotalSeconds
$timer=[Diagnostics.Stopwatch]::StartNew()
$working=@()
for($i=0;$i -lt 30;$i++){Start-Sleep -Seconds 1;$p.Refresh();$working+=$p.WorkingSet64}
$seconds=$timer.Elapsed.TotalSeconds
$cpu=$p.TotalProcessorTime.TotalSeconds-$cpuStart
$result=[pscustomobject]@{scope='empty P0 UI; no directory or player loaded';duration_seconds=$seconds;cpu_seconds=$cpu;logical_processors=[Environment]::ProcessorCount;normalized_cpu_percent=100*$cpu/$seconds/[Environment]::ProcessorCount;working_set_max_bytes=($working|Measure-Object -Maximum).Maximum;working_set_min_bytes=($working|Measure-Object -Minimum).Minimum;memory_samples=$working}
$result | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p0-idle.json" -Encoding utf8
$p.WaitForExit()
Get-Content "$taskRoot/artifacts/p0-idle.json"
