param([string]$ExecutablePath='dist/browser-preview/media-browser.exe',[string]$PythonPath='python')
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $ExecutablePath)).Path
$taskBase=Join-Path $taskRoot ('target/trash-ui-'+[DateTime]::Now.Ticks)
$taskMedia=Join-Path $taskBase 'media'
New-Item -ItemType Directory -Path $taskMedia | Out-Null
# Screenshots must be outside the watched fixture, otherwise the ancestor tree
# correctly counts diagnostic PNGs as additional media.
$taskWork=Join-Path $taskRoot ('artifacts/trash-ui-'+[DateTime]::Now.Ticks)
$taskEvidenceDir=Join-Path $taskWork 'artifacts'
New-Item -ItemType Directory -Path $taskEvidenceDir | Out-Null
Set-Content -LiteralPath (Join-Path $taskBase '.trash-smoke-owned') -Value 'generated recycle UI copies' -NoNewline -Encoding utf8
$taskImage=Join-Path $taskRoot 'test-media/ui-demo/封面.jpg'
$taskVideo=Join-Path $taskRoot 'test-media/test-h264-aac.mp4'
$taskGif=Join-Path $taskRoot 'test-media/gif-demo/循环.gif'
foreach($taskName in '01.jpg','02.jpg','05.jpg','06.jpg'){Copy-Item -LiteralPath $taskImage -Destination (Join-Path $taskMedia $taskName)}
Copy-Item -LiteralPath $taskVideo -Destination (Join-Path $taskMedia '03.mp4')
Copy-Item -LiteralPath $taskGif -Destination (Join-Path $taskMedia '04.gif')
$taskOriginals=@($taskImage,$taskVideo,$taskGif) | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash
$taskCopies=Get-ChildItem -LiteralPath $taskMedia -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash
$taskStarted=[DateTime]::UtcNow
$taskApp=Start-Process -FilePath $taskExe -ArgumentList '--trash-smoke','--open',('"'+$taskBase+'"'),'--instance-scope',([IO.Path]::GetFileName($taskBase)),'--data-dir',('"'+(Join-Path $taskBase 'data')+'"') -WorkingDirectory $taskWork -RedirectStandardOutput "$taskEvidenceDir/p4-trash-ui-stdout.log" -RedirectStandardError "$taskEvidenceDir/p4-trash-ui-stderr.log" -PassThru
if(-not $taskApp.WaitForExit(180000)){throw "Owned trash diagnostic still live; inspect PID $($taskApp.Id) before retrying"}
$taskEvidence=Get-Item -LiteralPath "$taskEvidenceDir/p4-trash-ui.json"
if($taskEvidence.LastWriteTimeUtc -lt $taskStarted){throw 'Missing fresh recycle UI evidence'}
$taskResult=Get-Content -LiteralPath $taskEvidence.FullName -Raw | ConvertFrom-Json
# Restore only exact receipt items under this run's marked generated media directory.
$taskReceipts=@($taskResult.reports)+@($taskResult.rows | ForEach-Object last_trash)
$taskItems=@($taskReceipts | Where-Object submitted | ForEach-Object items | Where-Object success | Sort-Object path -Unique)
$taskShell=New-Object -ComObject Shell.Application
$taskBin=$taskShell.NameSpace(10)
$taskRestored=@()
foreach($taskItem in $taskItems){
    $taskExpectedPath=[string]$taskItem.path
    if($taskExpectedPath.StartsWith('\\?\')){$taskExpectedPath=$taskExpectedPath.Substring(4)}
    $taskExpectedPath=[IO.Path]::GetFullPath($taskExpectedPath)
    if([IO.Path]::GetDirectoryName($taskExpectedPath) -ne $taskMedia -or -not (Test-Path -LiteralPath (Join-Path $taskBase '.trash-smoke-owned'))){throw 'Refuse restore outside this dedicated fixture'}
    $taskRecycledPath=[string]$taskItem.recycled_path
    if($taskRecycledPath.StartsWith('\\?\')){$taskRecycledPath=$taskRecycledPath.Substring(4)}
    $taskEntry=@($taskBin.Items() | Where-Object {$_.Path -eq $taskRecycledPath})
    if($taskEntry.Count -ne 1){throw "Missing exact owned recycle entry: $taskExpectedPath"}
    $taskEntry[0].InvokeVerb('undelete')
    $taskDeadline=[DateTime]::UtcNow.AddSeconds(5)
    while(-not (Test-Path -LiteralPath $taskExpectedPath) -and [DateTime]::UtcNow -lt $taskDeadline){Start-Sleep -Milliseconds 50}
    $taskOriginalHash=($taskCopies | Where-Object Path -eq $taskExpectedPath).Hash
    if(-not $taskOriginalHash -or (Get-FileHash -LiteralPath $taskExpectedPath -Algorithm SHA256).Hash -ne $taskOriginalHash){throw "Owned copy restore failed: $taskExpectedPath"}
    $taskRestored+=@{path=$taskExpectedPath;bytes_restored=$true}
}
$taskAfter=$taskOriginals.Path | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash
$taskCopiesAfter=Get-ChildItem -LiteralPath $taskMedia -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash
$taskRun=@{exit_code=$taskApp.ExitCode;started_utc=$taskStarted;ended_utc=[DateTime]::UtcNow;exe_path=$taskExe;exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash;fixture=$taskBase;originals_before=$taskOriginals;originals_after=$taskAfter;copies_before=$taskCopies;copies_after=$taskCopiesAfter;restored=$taskRestored}
$taskRun | ConvertTo-Json -Depth 8 | Set-Content "$taskEvidenceDir/p4-trash-ui-run.json" -Encoding utf8
if($taskApp.ExitCode -ne 0 -or $taskResult.error){throw "Recycle UI failed: $($taskResult.error)"}
& $PythonPath -X utf8 "$PSScriptRoot/verify_trash_ui.py" --evidence-dir $taskEvidenceDir
if($LASTEXITCODE -ne 0){throw 'Recycle UI verifier failed'}
Write-Output "Evidence directory: $taskEvidenceDir"
