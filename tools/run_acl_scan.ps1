# Only modifies the ACL of a freshly created dedicated fixture child; restores in finally.
param([string]$ExecutablePath='dist/browser-preview/media-browser.exe',[string]$Output='',[switch]$Gui)
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskFixture=Join-Path $taskRoot ('target/acl-scan-'+[guid]::NewGuid().ToString('N'))
$taskTargetRoot=[IO.Path]::GetFullPath((Join-Path $taskRoot 'target'))+[IO.Path]::DirectorySeparatorChar
if(-not [IO.Path]::GetFullPath($taskFixture).StartsWith($taskTargetRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Fixture outside target'}
[IO.Directory]::CreateDirectory($taskFixture) | Out-Null
[IO.File]::WriteAllText((Join-Path $taskFixture '.acl-fixture'),'Dedicated generated fixture; restore original ACL in finally.')
$taskMedia=Join-Path $taskFixture 'media'
[IO.Directory]::CreateDirectory($taskMedia) | Out-Null
$taskDenied=Join-Path $taskMedia '拒绝访问子目录'
$taskReadable=Join-Path $taskMedia '可读子目录'
[IO.Directory]::CreateDirectory($taskDenied) | Out-Null
[IO.Directory]::CreateDirectory($taskReadable) | Out-Null
$taskSource=Join-Path $taskRoot 'test-media/format-matrix-v1/png/sample.png'
Copy-Item -LiteralPath $taskSource -Destination (Join-Path $taskDenied 'sample.png')
Copy-Item -LiteralPath $taskSource -Destination (Join-Path $taskReadable 'sample.png')
$taskAcl=Get-Acl -LiteralPath $taskDenied
$taskSddl=$taskAcl.Sddl
[IO.File]::WriteAllText((Join-Path $taskFixture 'original-acl.sddl'),$taskSddl)
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $ExecutablePath)).Path
$taskDb=Join-Path $taskFixture 'index.sqlite3'
$taskEvidence=[ordered]@{fixture=$taskFixture;exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash;scope='Real NTFS denied directory on dedicated copies, existing index retained; no user media or global ACL changes';acl_restored=$false;checks_pass=$false}
function Invoke-FixtureScan([string]$phase) {
    $taskOut=Join-Path $taskFixture ($phase+'-stdout.json')
    $taskErr=Join-Path $taskFixture ($phase+'-stderr.log')
    $taskProc=Start-Process -FilePath $taskExe -ArgumentList @('--scan-report',('"'+$taskMedia+'"'),'--db',('"'+$taskDb+'"')) -WindowStyle Hidden -RedirectStandardOutput $taskOut -RedirectStandardError $taskErr -PassThru
    $taskHandle=$taskProc.Handle
    if(-not $taskProc.WaitForExit(65000)){throw "Scan still live PID $($taskProc.Id); inspect before restarting"}
    $taskProc.WaitForExit()
    if($taskProc.ExitCode -ne 0){throw "Scan $phase exit $($taskProc.ExitCode): $(Get-Content -LiteralPath $taskErr -Raw)"}
    [IO.File]::ReadAllText($taskOut,[Text.Encoding]::UTF8)
}
function Invoke-FixtureGui([string]$phase) {
    $taskData=Join-Path $taskFixture 'gui-data'
    if(-not (Test-Path -LiteralPath $taskData)){
        New-Item -ItemType Directory -Path $taskData | Out-Null
        Copy-Item -LiteralPath $taskDb -Destination (Join-Path $taskData 'index.sqlite')
    }
    $taskMetrics=Join-Path $taskFixture ($phase+'-gui.json')
    $taskProc=Start-Process -FilePath $taskExe -ArgumentList @('--open',('"'+$taskMedia+'"'),'--data-dir',('"'+$taskData+'"'),'--instance-scope',([IO.Path]::GetFileName($taskFixture)+'-'+$phase),'--load-metrics',('"'+$taskMetrics+'"'),'--quit-after','5000') -WorkingDirectory $taskFixture -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFixture ($phase+'-gui-stdout.log')) -RedirectStandardError (Join-Path $taskFixture ($phase+'-gui-stderr.log')) -PassThru
    $taskHandle=$taskProc.Handle
    if(-not $taskProc.WaitForExit(30000)){throw "Own GUI remains live PID $($taskProc.Id); inspect before restarting"}
    $taskProc.WaitForExit()
    if($taskProc.ExitCode -ne 0){throw "GUI $phase exit $($taskProc.ExitCode)"}
    $taskDraw=Get-Content -LiteralPath $taskMetrics -Raw -Encoding utf8 | ConvertFrom-Json
    $taskReady=@($taskDraw.rows | Where-Object {$_.visible_ready -gt 0} | Select-Object -First 1)
    $taskPng=[IO.Path]::ChangeExtension($taskMetrics,'png')
    if(-not $taskReady.Count -or -not (Test-Path -LiteralPath $taskPng)){throw 'No actual ready-texture FBO screenshot'}
    return [pscustomobject]@{pid=$taskProc.Id;exit_code=$taskProc.ExitCode;metrics=$taskMetrics;screenshot=$taskPng;screenshot_sha256=(Get-FileHash -LiteralPath $taskPng -Algorithm SHA256).Hash;first_ready=$taskReady[0];scope='Native callback and first-ready GL screenshot; status text requires visual inspection, not physical input'}
}
$taskBefore=Invoke-FixtureScan 'before' 
$taskEvidence.before=($taskBefore -join "`n") | ConvertFrom-Json
$taskSid=[Security.Principal.WindowsIdentity]::GetCurrent().User
$taskRule=[Security.AccessControl.FileSystemAccessRule]::new($taskSid,[Security.AccessControl.FileSystemRights]::ListDirectory,[Security.AccessControl.AccessControlType]::Deny)
try {
    $taskModified=Get-Acl -LiteralPath $taskDenied
    $taskModified.AddAccessRule($taskRule)
    Set-Acl -LiteralPath $taskDenied -AclObject $taskModified
    $taskReadDenied=$false
    try {Get-ChildItem -LiteralPath $taskDenied -ErrorAction Stop | Out-Null} catch [UnauthorizedAccessException] {$taskReadDenied=$true}
    $taskEvidence.actual_enumeration_denied=$taskReadDenied
    if(-not $taskReadDenied){throw 'No actual denied access; test environment does not establish denial'}
    $taskDuring=Invoke-FixtureScan 'during'
    $taskEvidence.during=($taskDuring -join "`n") | ConvertFrom-Json
    if($Gui){$taskEvidence.gui_during=Invoke-FixtureGui 'during'}
} finally {
    try {
        Set-Acl -LiteralPath $taskDenied -AclObject $taskAcl
        $taskEvidence.acl_restored=((Get-Acl -LiteralPath $taskDenied).Sddl -eq $taskSddl)
    } catch {
        $taskEvidence.restore_error=$_.Exception.Message
        throw
    } finally {
        $taskEvidence | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $taskFixture 'result.json') -Encoding utf8
    }
}
$taskAfter=Invoke-FixtureScan 'after'
$taskEvidence.after=($taskAfter -join "`n") | ConvertFrom-Json
if($Gui){$taskEvidence.gui_after=Invoke-FixtureGui 'after'}
$taskEvidence.source_hashes_unchanged=((Get-FileHash -LiteralPath (Join-Path $taskDenied 'sample.png')).Hash -eq (Get-FileHash -LiteralPath $taskSource).Hash) -and ((Get-FileHash -LiteralPath (Join-Path $taskReadable 'sample.png')).Hash -eq (Get-FileHash -LiteralPath $taskSource).Hash)
$taskPartial=@($taskEvidence.during.directories | Where-Object {-not $_.subtree_complete})
$taskEvidence.checks_pass=$taskEvidence.acl_restored -and $taskEvidence.actual_enumeration_denied -and $taskEvidence.source_hashes_unchanged -and ($taskEvidence.before.media_count -eq 2) -and ($taskEvidence.during.media_count -eq 2) -and ($taskEvidence.after.media_count -eq 2) -and ($taskEvidence.during.errors.Count -gt 0) -and ($taskPartial.Count -gt 0) -and ($taskEvidence.after.errors.Count -eq 0) -and ($taskEvidence.after.integrity_check -eq 'ok')
$taskOutput=Join-Path $taskFixture 'result.json'
$taskEvidence | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $taskOutput -Encoding utf8
[pscustomobject]@{evidence=$taskOutput;checks_pass=$taskEvidence.checks_pass;acl_restored=$taskEvidence.acl_restored;during_media=$taskEvidence.during.media_count;errors=$taskEvidence.during.errors;partial_directories=$taskPartial.Count} | ConvertTo-Json -Depth 4
if($Output){Copy-Item -LiteralPath $taskOutput -Destination (Join-Path $taskRoot $Output)}
if(-not $taskEvidence.checks_pass){throw 'ACL scan acceptance checks failed; inspect saved evidence'}
