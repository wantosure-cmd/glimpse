param([string]$ExecutablePath='')
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
if($ExecutablePath){$taskExecutable=(Resolve-Path -LiteralPath $ExecutablePath).Path}
else { & "$PSScriptRoot/package_preview.ps1"; $taskExecutable="$taskRoot/dist/browser-preview/media-browser.exe" }
$taskBase='target/watch-ui-'+[DateTime]::Now.Ticks
$taskMedia=Join-Path $taskBase 'media'
New-Item -ItemType Directory -Path (Join-Path $taskRoot $taskMedia) -Force | Out-Null
Set-Content -LiteralPath (Join-Path $taskRoot "$taskBase/.watch-smoke-owned") -Value 'generated watch UI fixture' -NoNewline -Encoding utf8
$taskSource=Join-Path $taskRoot 'test-media/ui-demo/封面.jpg'
Copy-Item -LiteralPath $taskSource -Destination (Join-Path $taskRoot "$taskBase/original.jpg")
0..39 | ForEach-Object { Copy-Item -LiteralPath $taskSource -Destination (Join-Path $taskRoot ($taskMedia+'/{0:D3}.jpg' -f $_)) }
$taskHashes=@($taskSource,(Join-Path $taskRoot "$taskBase/original.jpg"),(Join-Path $taskRoot "test-media/test-h264-aac.mp4")) | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash
$taskHashes | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p4-watch-ui-originals-before.json" -Encoding utf8
$taskStarted=[DateTime]::UtcNow
[pscustomobject]@{checks_pass=$false;state='running'} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p4-watch-ui-verification.json" -Encoding utf8
$taskApp=Start-Process -FilePath $taskExecutable -ArgumentList '--watch-smoke','--open',$taskMedia,'--data-dir',"$taskBase/data" -WorkingDirectory $taskRoot -RedirectStandardOutput "$taskRoot/artifacts/p4-watch-ui-stdout.log" -RedirectStandardError "$taskRoot/artifacts/p4-watch-ui-stderr.log" -PassThru
$taskApp.WaitForExit()
$taskHashes.Path | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p4-watch-ui-originals-after.json" -Encoding utf8
[pscustomobject]@{exit_code=$taskApp.ExitCode;started_utc=$taskStarted;ended_utc=[DateTime]::UtcNow;exe_path=$taskExecutable;exe_sha256=(Get-FileHash -LiteralPath $taskExecutable -Algorithm SHA256).Hash;fixture=$taskBase} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p4-watch-ui-run.json" -Encoding utf8
if((Get-Item -LiteralPath "$taskRoot/artifacts/p4-watch-ui.json").LastWriteTimeUtc -lt $taskStarted){throw 'Missing fresh watch UI report'}
$taskReport=Get-Content -LiteralPath "$taskRoot/artifacts/p4-watch-ui.json" -Raw | ConvertFrom-Json
if($taskReport.error){throw "Native watch UI failed: $($taskReport.error)"}
foreach($taskName in @('p4-watch-ui.json','p4-watch-added.png','p4-watch-replaced.png')) {
    if((Get-Item -LiteralPath "$taskRoot/artifacts/$taskName").LastWriteTimeUtc -lt $taskStarted){throw "Missing fresh evidence: $taskName"}
}
python -X utf8 "$PSScriptRoot/verify_watch_smoke.py"
if($LASTEXITCODE -ne 0){throw 'Native watch UI verification failed'}
