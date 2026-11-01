param(
 [string]$ExecutablePath='dist/0.2.0-local-test-20261021-responsive/media-browser-portable/media-browser.exe',
 [string]$Output='artifacts/isolated-instance-check.json'
)
$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $ExecutablePath)).Path
$taskOutput=[IO.Path]::GetFullPath((Join-Path $taskRoot $Output))
if(-not $taskOutput.StartsWith($taskRoot.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Report must remain inside workspace'}
if(-not ('MediaBrowserTokenProbe' -as [type])){Add-Type -Path (Join-Path $PSScriptRoot 'token-restriction-probe.cs')}
$taskScope='isolated-ipc-'+[guid]::NewGuid().ToString('N')
$taskBase=Join-Path $taskRoot ('target/'+$taskScope)
New-Item -ItemType Directory -Path $taskBase | Out-Null
Set-Content -LiteralPath (Join-Path $taskBase '.isolated-ipc-owned') -Value 'generated non-destructive instance check' -NoNewline -Encoding ASCII
$taskA=Join-Path $taskBase 'A 初始目录'
$taskB=Join-Path $taskBase 'B 中文 空格'
foreach($taskFolder in @($taskA,$taskB)){
 New-Item -ItemType Directory -Path $taskFolder | Out-Null
 Copy-Item -LiteralPath (Join-Path $taskRoot 'test-media/ui-demo/封面.jpg') -Destination (Join-Path $taskFolder '生成副本.jpg')
}
$taskBefore=@(Get-ChildItem -LiteralPath $taskA,$taskB -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash)
function Quote-TaskPath([string]$Path){return '"'+$Path+'"'}
function Read-TaskReport([string]$Path){try{if(Test-Path -LiteralPath $Path){return Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json}}catch{};return $null}
function Plain-TaskPath([string]$Path){if($Path.StartsWith('\\?\UNC\')){$Path='\\'+$Path.Substring(8)}elseif($Path.StartsWith('\\?\')){$Path=$Path.Substring(4)};return [IO.Path]::GetFullPath($Path).TrimEnd('\')}
$taskReport=Join-Path $taskBase 'server.json'
$taskServer=Start-Process -FilePath $taskExe -ArgumentList '--instance-scope',$taskScope,'--instance-report',(Quote-TaskPath $taskReport),'--open',(Quote-TaskPath $taskA),'--data-dir',(Quote-TaskPath (Join-Path $taskBase 'data')),'--quit-after','12000' -WorkingDirectory $taskRoot -RedirectStandardOutput (Join-Path $taskBase 'server-stdout.log') -RedirectStandardError (Join-Path $taskBase 'server-stderr.log') -PassThru
$taskServerHandle=$taskServer.Handle
function Trace-TaskStage([string]$Stage){([DateTime]::UtcNow.ToString('o')+' '+$Stage) | Add-Content -LiteralPath (Join-Path $taskBase 'stages.log') -Encoding UTF8}
Trace-TaskStage 'server-started'
$taskServerToken=$null;$taskTokenError=$null
try{$taskServerToken=[MediaBrowserTokenProbe]::ReadPid($taskServer.Id)}catch{$taskTokenError=$_.Exception.Message}
Trace-TaskStage 'server-token-read'
$taskBeforeReport=$null;$taskUntil=[DateTime]::UtcNow.AddSeconds(5)
while([DateTime]::UtcNow -lt $taskUntil -and -not $taskServer.HasExited){
 $taskBeforeReport=Read-TaskReport $taskReport
 if($taskBeforeReport -and @($taskBeforeReport.rows | Where-Object {$_.media -eq 1}).Count -gt 0){break}
 Start-Sleep -Milliseconds 50
}
$taskClient=Start-Process -FilePath $taskExe -ArgumentList '--instance-scope',$taskScope,'--open',(Quote-TaskPath $taskB) -WorkingDirectory $taskRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskBase 'client-stdout.json') -RedirectStandardError (Join-Path $taskBase 'client-stderr.log') -PassThru
$taskClientHandle=$taskClient.Handle
Trace-TaskStage 'client-started'
if(-not $taskClient.WaitForExit(10000)){throw "Own client still running: PID $($taskClient.Id); inspect, do not restart"};[void]$taskClient.WaitForExit(1000)
Trace-TaskStage 'client-exited'
if(-not $taskServer.WaitForExit(20000)){throw "Own server still running: PID $($taskServer.Id); inspect, do not restart"};[void]$taskServer.WaitForExit(1000)
Trace-TaskStage 'server-exited'
$taskAfter=Read-TaskReport $taskReport
Trace-TaskStage 'after-report-read'
# File.ReadAllText returns a plain string. PS5.1 Get-Content strings carry PSDrive/PSProvider metadata that ConvertTo-Json can expand into a huge object graph.
$taskClientOut=[IO.File]::ReadAllText((Join-Path $taskBase 'client-stdout.json'),[Text.Encoding]::UTF8)
$taskClientResult=$null;try{$taskClientResult=$taskClientOut | ConvertFrom-Json}catch{}
$taskMatched=@();if($taskAfter){$taskMatched=@($taskAfter.rows | Where-Object {$_.received -ge 1 -and $_.media -eq 1 -and (Plain-TaskPath $_.root).Equals((Plain-TaskPath $taskB),[StringComparison]::OrdinalIgnoreCase)})}
$taskUnchanged=$true
foreach($taskFile in $taskBefore){if((Get-FileHash -LiteralPath $taskFile.Path -Algorithm SHA256).Hash -ne $taskFile.Hash){$taskUnchanged=$false}}
$taskPass=$taskClient.ExitCode -eq 0 -and $taskServer.ExitCode -eq 0 -and $taskClientResult -and $taskClientResult.forwarded -and $taskClientResult.server_pid -eq $taskServer.Id -and $taskMatched.Count -gt 0 -and $taskUnchanged
Trace-TaskStage 'sources-and-handoff-checked'
# Normalize native token records to stable primitive fields for the report.
function Plain-TaskToken($Token){
 if($null -eq $Token){return $null}
 return [pscustomobject]@{restricted=[bool]$Token.restricted;restricted_sid_count=[int]$Token.restricted_sid_count;restricted_sids=@($Token.restricted_sids | ForEach-Object {[string]$_});integrity_sid=[string]$Token.integrity_sid;scope=[string]$Token.scope}
}
$taskResult=[pscustomobject]@{
 recorded_utc=[DateTime]::UtcNow.ToString('o');exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash
 checks_pass=[bool]$taskPass;full_acceptance_pass=$false;fixture=$taskBase
 caller_token=(Plain-TaskToken ([MediaBrowserTokenProbe]::Read()));server_token=(Plain-TaskToken $taskServerToken);server_token_query_error=$taskTokenError
 server_pid=$taskServer.Id;server_exit_code=$taskServer.ExitCode;client_pid=$taskClient.Id;client_exit_code=$taskClient.ExitCode
 client_stdout=$taskClientOut;client_stderr=[IO.File]::ReadAllText((Join-Path $taskBase 'client-stderr.log'),[Text.Encoding]::UTF8)
 before=$taskBeforeReport;after=$taskAfter;generated_copy_hashes_unchanged=$taskUnchanged
 scope='Two isolated normal app processes; A/B one generated JPEG each; no registry/ACL/token changes, no user instance contacted or terminated, no deletion; not physical Shell menu or Win11 proof'
}
Trace-TaskStage 'result-created'
New-Item -ItemType Directory -Path (Split-Path $taskOutput) -Force | Out-Null
$taskResult | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $taskOutput -Encoding UTF8
Trace-TaskStage 'result-written'
Write-Output "Report: $taskOutput"
Write-Output "Same-instance handoff: $taskPass"
if(-not $taskPass){throw 'Handoff not verified; saved exact token and error evidence'}
