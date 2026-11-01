param(
    [Parameter(Mandatory=$true)][string]$Directory,
    [string]$ExecutablePath=(Join-Path $PSScriptRoot 'media-browser.exe'),
    [string]$OutputRoot='',
    [int]$Seconds=45
)
$ErrorActionPreference='Stop'
if($Seconds -lt 10 -or $Seconds -gt 180){throw 'Seconds must be 10..180'}
$taskExe=(Resolve-Path -LiteralPath $ExecutablePath).Path
if(-not $OutputRoot){$OutputRoot=Join-Path $env:LOCALAPPDATA ('GlimpseDiagnostics/load-'+[guid]::NewGuid().ToString('N'))}
$taskOutput=[IO.Path]::GetFullPath($OutputRoot)
if(Test-Path -LiteralPath $taskOutput){throw 'Output must be a new directory'}
New-Item -ItemType Directory -Path $taskOutput | Out-Null
$taskResult=[ordered]@{exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash;directory=$Directory;output=$taskOutput;completed=$false;scope='Read-only library open and native draw/ready-render telemetry; not physical presentation timing. Hover/click a video to measure playback. Private screenshots and local index remain in this output.'}
try{
    $taskDirectory=(Get-Item -LiteralPath $Directory -ErrorAction Stop).FullName
    $taskMetrics=Join-Path $taskOutput 'draw-metrics.json'
    $taskArgs=@('--open',('"'+$taskDirectory+'"'),'--data-dir',('"'+(Join-Path $taskOutput 'data')+'"'),'--instance-scope',('load-'+[guid]::NewGuid().ToString('N')),'--load-metrics',('"'+$taskMetrics+'"'),'--quit-after',($Seconds*1000))
    $taskApp=Start-Process -FilePath $taskExe -ArgumentList $taskArgs -WorkingDirectory $taskOutput -PassThru -RedirectStandardOutput (Join-Path $taskOutput 'stdout.log') -RedirectStandardError (Join-Path $taskOutput 'stderr.log')
    # Keep the native handle alive: Windows PowerShell can otherwise lose ExitCode.
    $taskHandle=$taskApp.Handle
    $taskResult['pid']=$taskApp.Id
    if(-not $taskApp.WaitForExit(($Seconds+30)*1000)){throw ('Diagnostic remains live: '+$taskApp.Id+'; do not start a duplicate')}
    $taskApp.WaitForExit();$taskResult['exit_code']=$taskApp.ExitCode
    if($taskApp.ExitCode -ne 0){throw 'Application failed; see stderr.log'}
    $taskDraw=Get-Content -LiteralPath $taskMetrics -Raw -Encoding UTF8 | ConvertFrom-Json
    $taskFirst=@($taskDraw.rows | Where-Object {$_.visible_ready -gt 0} | Select-Object -First 1)
    $taskPlay=@($taskDraw.rows | Where-Object {$null -ne $_.video_first_ready_render_ms} | Select-Object -First 1)
    $taskResult['version']=$taskDraw.version
    $taskResult['first_native_preview_ms']=$(if($taskFirst.Count){$taskFirst[0].elapsed_ms}else{$null})
    $taskResult['first_playback_ready_render_ms']=$(if($taskPlay.Count){$taskPlay[0].video_first_ready_render_ms}else{$null})
    $taskResult['completed']=$true
}catch{$taskResult['error']=$_.Exception.Message}
$taskResult | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $taskOutput 'result.json') -Encoding UTF8
Write-Host ('Report: '+(Join-Path $taskOutput 'result.json'))
if(-not $taskResult.completed){exit 1}
