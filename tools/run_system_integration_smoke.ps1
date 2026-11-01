param([string]$ExecutablePath='dist/browser-preview/media-browser.exe')
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskSource=(Resolve-Path -LiteralPath (Join-Path $taskRoot $ExecutablePath)).Path
$taskScope='system-'+[DateTime]::Now.Ticks
$taskBase=Join-Path $taskRoot ('target/'+$taskScope)
$taskCopyDir=Join-Path $taskBase '含 空格 程序'
New-Item -ItemType Directory -Path $taskCopyDir | Out-Null
Set-Content -LiteralPath (Join-Path $taskBase '.system-smoke-owned') -Value 'generated shell and instance fixture' -NoNewline -Encoding utf8
$taskExe=Join-Path $taskCopyDir '媒体 Browser.exe'
Copy-Item -LiteralPath $taskSource -Destination $taskExe
New-Item -ItemType Directory -Path (Join-Path $taskCopyDir 'runtime') | Out-Null
# Only needed for optional filename font fallback; this fixture contains JPEGs only.
Copy-Item -LiteralPath (Join-Path $taskRoot 'vendor/runtime/Chinese-full.ttf') -Destination (Join-Path $taskCopyDir 'runtime/Chinese-full.ttf')
$taskFolders=@('A 初始目录','B 中文 空格 🖼️','C 右键 图集','临时根目录') | ForEach-Object {Join-Path $taskBase $_}
foreach($taskFolder in $taskFolders){New-Item -ItemType Directory -Path $taskFolder | Out-Null;Copy-Item -LiteralPath (Join-Path $taskRoot 'test-media/ui-demo/封面.jpg') -Destination (Join-Path $taskFolder '一张图片.jpg')}
$taskBefore=Get-ChildItem -LiteralPath $taskFolders -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash
$taskReport=Join-Path $taskBase 'instance.json'
$taskClientRuns=@()
$taskRegistration=@()
$taskRows=@()
$taskDrive=$null
$taskDriveMapped=$false
$taskDriveRemoved=$false
$taskResult=$null
$taskPrimary=$null
$taskNewExe=Join-Path $taskCopyDir '新版媒体 Browser.exe'
$taskRegistryPaths=@('Directory','Directory\Background','Drive') | ForEach-Object {"Registry::HKEY_CURRENT_USER\Software\Classes\$_\shell\MediaBrowser.Test-$taskScope"}
foreach($taskRegistryPath in $taskRegistryPaths){if(Test-Path -LiteralPath $taskRegistryPath){throw 'Refusing existing test verb'}}
function Invoke-OwnedCli([string]$Exe,[string[]]$Arguments,[string]$Label){
    $taskOut=Join-Path $taskBase ($Label+'-stdout.json');$taskErr=Join-Path $taskBase ($Label+'-stderr.log')
    $taskP=Start-Process -FilePath $Exe -ArgumentList $Arguments -WindowStyle Hidden -RedirectStandardOutput $taskOut -RedirectStandardError $taskErr -PassThru
    if(-not $taskP.WaitForExit(10000)){throw "Owned CLI still running, PID $($taskP.Id): $Label"}
    $taskP.WaitForExit()
    return @{pid=$taskP.Id;exit_code=$taskP.ExitCode;stdout=(Get-Content -LiteralPath $taskOut -Raw);stderr=(Get-Content -LiteralPath $taskErr -Raw);label=$Label}
}
function Wait-OwnedRoot([int]$Received,[string]$Expected){
    $taskUntil=[DateTime]::UtcNow.AddSeconds(10)
    do{
        if(Test-Path -LiteralPath $taskReport){
            try{$taskState=Get-Content -LiteralPath $taskReport -Raw | ConvertFrom-Json}catch{$taskState=$null}
            if($taskState -and $taskState.pid -ne $taskPrimary.Id){throw 'Report PID changed'}
            $taskLatest=$taskState.rows | Select-Object -Last 1
            if($taskLatest -and $taskLatest.received -eq $Received){
                $taskActual=[string]$taskLatest.root
                if($taskActual.StartsWith('\\?\')){$taskActual=$taskActual.Substring(4)}
                $taskActual=$taskActual.TrimEnd('\')
                if($taskActual -eq $Expected.TrimEnd('\') -and $taskLatest.media -eq 1 -and ($Received -eq 0 -or -not $taskLatest.minimized)){return $taskLatest}
            }
        }
        Start-Sleep -Milliseconds 50
    }while([DateTime]::UtcNow -lt $taskUntil)
    throw "Missing expected root $Expected at received=$Received; PID $($taskPrimary.Id)"
}
Add-Type -TypeDefinition 'using System; using System.Text; using System.Runtime.InteropServices; public static class MediaBrowserDosDeviceAudit { [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern uint QueryDosDevice(string name, StringBuilder target, int size); }'
$taskStarted=[DateTime]::UtcNow
try{
    # A free temporary drive maps only to an owned one-file directory, never a user disk root.
    foreach($taskLetter in 'R','S','T','U','V','W','X','Y','Z'){
        if(-not (Test-Path -LiteralPath ($taskLetter+':\')) -and -not (Get-PSDrive -Name $taskLetter -ErrorAction SilentlyContinue)){$taskDrive=$taskLetter+':';break}
    }
    if(-not $taskDrive){throw 'No unused diagnostic drive letter'}
    & subst.exe $taskDrive $taskFolders[3]
    if($LASTEXITCODE -ne 0){throw 'Temporary drive mapping failed'}
    $taskDriveMapped=$true
    $taskDriveRoot=$taskDrive+'\'
    $taskRegister=Invoke-OwnedCli $taskExe @('--register-test-menu',$taskScope) 'register'
    if($taskRegister.exit_code -ne 0){throw $taskRegister.stderr};$taskRegistration+=$taskRegister
    $taskEntries=@($taskRegister.stdout | ConvertFrom-Json)
    $taskRegistryReadback=@($taskRegistryPaths | ForEach-Object {
        $taskValues=Get-ItemProperty -LiteralPath $_
        @{path=$_;properties=@{owner=[string]$taskValues.MediaBrowserOwner;executable=[string]$taskValues.MediaBrowserExecutable;label=[string]$taskValues.MUIVerb;icon=[string]$taskValues.Icon;selection=[string]$taskValues.MultiSelectModel};command=[string](Get-ItemProperty -LiteralPath ($_+'\command')).'(default)'}
    })
    $taskPrimary=Start-Process -FilePath $taskExe -ArgumentList '--instance-scope',$taskScope,'--instance-report',('"'+$taskReport+'"'),'--open',('"'+$taskFolders[0]+'"'),'--data-dir',('"'+(Join-Path $taskBase 'data')+'"'),'--quit-after','45000','--instance-minimize-after-open' -PassThru
    $taskRows+=Wait-OwnedRoot 0 $taskFolders[0]
    $taskMinimizedDeadline=[DateTime]::UtcNow.AddSeconds(5)
    do{
        $taskVisibility=(Get-Content -LiteralPath $taskReport -Raw | ConvertFrom-Json).visibility
        if(@($taskVisibility | Where-Object minimized).Count -gt 0){break}
        Start-Sleep -Milliseconds 50
    }while([DateTime]::UtcNow -lt $taskMinimizedDeadline)
    if(@($taskVisibility | Where-Object minimized).Count -eq 0){throw 'Native backend never reported actual minimized state'}

    $taskClient=Invoke-OwnedCli $taskExe @('--instance-scope',$taskScope,'--open',('"'+$taskFolders[1]+'"')) 'forward-chinese';$taskClientRuns+=$taskClient
    if($taskClient.exit_code -ne 0){throw $taskClient.stderr};$taskRows+=Wait-OwnedRoot 1 $taskFolders[1]
    $taskShellDirectory=Start-Process -FilePath $taskFolders[2] -Verb ('MediaBrowser.Test-'+$taskScope) -WindowStyle Hidden -PassThru
    if(-not $taskShellDirectory.WaitForExit(10000)){throw 'Shell directory client did not exit'};$taskShellDirectory.WaitForExit();if($taskShellDirectory.ExitCode -ne 0){throw 'Shell directory verb failed'}
    $taskRows+=Wait-OwnedRoot 2 $taskFolders[2]
    $taskShellDrive=Start-Process -FilePath $taskDriveRoot -Verb ('MediaBrowser.Test-'+$taskScope) -WindowStyle Hidden -PassThru
    if(-not $taskShellDrive.WaitForExit(10000)){throw 'Shell drive client did not exit'};$taskShellDrive.WaitForExit();if($taskShellDrive.ExitCode -ne 0){throw 'Shell drive verb failed'}
    $taskRows+=Wait-OwnedRoot 3 $taskFolders[3]
    $taskBackground=($taskEntries | Where-Object registry_path -Like '*\Directory\Background\shell\*').command
    $taskArguments=$taskBackground.Substring(('"'+$taskExe+'"').Length).Replace('%V',$taskFolders[0])
    $taskClient=Invoke-OwnedCli $taskExe @($taskArguments) 'background-expanded-folder';$taskClientRuns+=$taskClient
    if($taskClient.exit_code -ne 0){throw $taskClient.stderr};$taskRows+=Wait-OwnedRoot 4 $taskFolders[0]
    $taskArguments=$taskBackground.Substring(('"'+$taskExe+'"').Length).Replace('%V',$taskDriveRoot)
    $taskClient=Invoke-OwnedCli $taskExe @($taskArguments) 'background-expanded-root';$taskClientRuns+=$taskClient
    if($taskClient.exit_code -ne 0){throw $taskClient.stderr};$taskRows+=Wait-OwnedRoot 5 $taskFolders[3]
    $taskClient=Invoke-OwnedCli $taskExe @('--instance-scope',$taskScope) 'activate-only';$taskClientRuns+=$taskClient
    if($taskClient.exit_code -ne 0){throw $taskClient.stderr};$taskRows+=Wait-OwnedRoot 6 $taskFolders[3]
    $taskClient=Invoke-OwnedCli $taskExe @('--instance-scope',$taskScope,'--open',('"'+(Join-Path $taskFolders[0] '一张图片.jpg')+'"')) 'reject-file';$taskClientRuns+=$taskClient
    if($taskClient.exit_code -eq 0){throw 'Accepted a file as directory'}
    Copy-Item -LiteralPath $taskExe -Destination $taskNewExe
    $taskUpgrade=Invoke-OwnedCli $taskNewExe @('--register-test-menu',$taskScope) 'upgrade';$taskRegistration+=$taskUpgrade
    if($taskUpgrade.exit_code -ne 0){throw $taskUpgrade.stderr}
    $taskOldRemove=Invoke-OwnedCli $taskExe @('--unregister-test-menu',$taskScope) 'old-install-unregister';$taskRegistration+=$taskOldRemove
    $taskPreserved=$taskOldRemove.stdout | ConvertFrom-Json
    if($taskPreserved.removed.Count -ne 0 -or $taskPreserved.preserved.Count -ne 3){throw 'Old install removed new registration'}
    $taskNewRemove=Invoke-OwnedCli $taskNewExe @('--unregister-test-menu',$taskScope) 'new-install-unregister';$taskRegistration+=$taskNewRemove
    if($taskNewRemove.exit_code -ne 0){throw $taskNewRemove.stderr}
    foreach($taskRegistryPath in $taskRegistryPaths){if(Test-Path -LiteralPath $taskRegistryPath){throw 'Owned menu key left after unregister'}}
    if(-not $taskPrimary.WaitForExit(55000)){throw "Primary still running: PID $($taskPrimary.Id)"};$taskPrimary.WaitForExit();if($taskPrimary.ExitCode -ne 0){throw 'Primary failed'}
    $taskReopen=Start-Process -FilePath $taskExe -ArgumentList '--instance-scope',$taskScope,'--instance-report',('"'+(Join-Path $taskBase 'reopened.json')+'"'),'--open',('"'+$taskFolders[0]+'"'),'--data-dir',('"'+(Join-Path $taskBase 'data2')+'"'),'--quit-after','1800' -PassThru
    if(-not $taskReopen.WaitForExit(10000)){throw 'Reopened primary did not exit'};$taskReopen.WaitForExit()
    $taskResult=@{started_utc=$taskStarted;ended_utc=[DateTime]::UtcNow;exe_path=$taskSource;exe_sha256=(Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash;copied_exe=$taskExe;copied_exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash;fixture=$taskBase;primary_pid=$taskPrimary.Id;primary_exit_code=$taskPrimary.ExitCode;visibility=(Get-Content -LiteralPath $taskReport -Raw | ConvertFrom-Json).visibility;rows=$taskRows;clients=$taskClientRuns;registration=$taskRegistration;registry_readback=$taskRegistryReadback;registry_removed=$true;reopened=(Get-Content -LiteralPath (Join-Path $taskBase 'reopened.json') -Raw | ConvertFrom-Json);originals_before=$taskBefore;originals_after=(Get-ChildItem -LiteralPath $taskFolders -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash);directory_shell_exit=$taskShellDirectory.ExitCode;drive_shell_exit=$taskShellDrive.ExitCode;background_scope='Expanded actual registered command through direct CreateProcess, not Explorer IContextMenu input';input_scope='Real ShellExecute static verb and native IPC, no physical menu click or Win11 proof'}

}finally{
    if($taskPrimary -and -not $taskPrimary.HasExited){[void]$taskPrimary.WaitForExit(55000)}
    if(Test-Path -LiteralPath $taskNewExe){[void](Invoke-OwnedCli $taskNewExe @('--unregister-test-menu',$taskScope) 'cleanup-new')}
    [void](Invoke-OwnedCli $taskExe @('--unregister-test-menu',$taskScope) 'cleanup-old')
    if($taskDriveMapped){
        $taskTarget=[Text.StringBuilder]::new(32768)
        $taskLength=[MediaBrowserDosDeviceAudit]::QueryDosDevice($taskDrive,$taskTarget,$taskTarget.Capacity)
        if($taskLength -gt 0 -and $taskTarget.ToString() -eq ('\??\'+$taskFolders[3])){& subst.exe $taskDrive /D}
        else{throw 'Drive mapping changed; refusing removal'}
        $taskTargetAfter=[Text.StringBuilder]::new(32768)
        $taskDriveRemoved=([MediaBrowserDosDeviceAudit]::QueryDosDevice($taskDrive,$taskTargetAfter,$taskTargetAfter.Capacity) -eq 0)
        if(-not $taskDriveRemoved){throw 'Owned temporary drive still mapped'}
    }
}
$taskResult.drive_removed=$taskDriveRemoved
$taskResult | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $taskRoot 'artifacts/p4-system-integration.json') -Encoding utf8
python -X utf8 "$PSScriptRoot/verify_system_integration.py"
if($LASTEXITCODE -ne 0){throw 'System integration verification failed'}
