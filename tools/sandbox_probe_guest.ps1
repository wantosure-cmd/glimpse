# Runs only in our disposable Windows Sandbox. Never run on the host.
$ErrorActionPreference='Stop'
if($env:USERNAME -ine 'WDAGUtilityAccount'){throw 'Refusing to run outside Windows Sandbox account'}
$taskInput='C:\MediaProbeInput';$taskOutput='C:\MediaProbeOutput'
$taskManifest=Get-Content -LiteralPath ($taskInput+'\manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if($env:COMPUTERNAME -ieq $taskManifest.host_computer){throw 'Refusing to run on host computer'}
$taskState=[ordered]@{token=$taskManifest.token;started_utc=[DateTime]::UtcNow.ToString('o');state='started';checks_pass=$false;scope='Disposable offline Windows Sandbox portable/installed dependencies, HEIC, null MOV seek, install/uninstall and native PNG GPU probe; not full product/physical-input acceptance'}
function Save-Probe {$taskState | ConvertTo-Json -Depth 14 | Set-Content -LiteralPath ($taskOutput+'\result.json') -Encoding UTF8}
function Require([bool]$Condition,[string]$Message){if(-not $Condition){throw $Message}}
function Run-Probe([string]$Name,[string]$Exe,[string[]]$Arguments){
    $taskStart=@{FilePath=$Exe;WorkingDirectory='C:\GuestMediaProbe';RedirectStandardOutput=($taskOutput+'\'+$Name+'-stdout.log');RedirectStandardError=($taskOutput+'\'+$Name+'-stderr.log');PassThru=$true}
    if($Arguments.Count -gt 0){$taskStart.ArgumentList=$Arguments}
    $p=Start-Process @taskStart
    $handle=$p.Handle
    if(-not $p.WaitForExit(65000)){$taskState.live_guest_pid=$p.Id;throw ('Owned guest diagnostic still live: '+$p.Id)}
    $p.WaitForExit();Require ($p.ExitCode -eq 0) ($Name+' exited '+$p.ExitCode)
}
Save-Probe
try {
    $taskState.account=$env:USERNAME;$taskState.computer=$env:COMPUTERNAME
    $taskState.os=Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber
    $taskState.machine=Get-CimInstance Win32_ComputerSystem | Select-Object Manufacturer,Model,TotalPhysicalMemory
    $taskState.gpu=@(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion)
    $taskState.default_routes=@(Get-NetRoute -DestinationPrefix '0.0.0.0/0','::/0' -ErrorAction SilentlyContinue | Select-Object InterfaceAlias,DestinationPrefix,NextHop)
    $taskState.path_development_tools=@(Get-Command python,python3,cargo,rustc,node,ffmpeg,ffprobe -ErrorAction SilentlyContinue | Select-Object Name,Source)
    $taskState.heic_store_extensions=@(@('*HEIF*','*HEVC*') | ForEach-Object {Get-AppxPackage -Name $_ -ErrorAction SilentlyContinue} | Select-Object Name,Version)
    Save-Probe
    Require ($taskState.default_routes.Count -eq 0) 'Guest has a default network route despite offline configuration'
    Require (-not (Test-Path -LiteralPath 'C:\SandboxMediaApp')) 'Guest application directory already exists'
    Copy-Item -LiteralPath 'C:\MediaProbeProduct' -Destination 'C:\SandboxMediaApp' -Recurse
    New-Item -ItemType Directory -Path 'C:\GuestMediaProbe' | Out-Null
    $taskExe='C:\SandboxMediaApp\media-browser.exe'
    Require ((Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash -eq $taskManifest.exe_sha256) 'Application hash mismatch'
    $taskFiles=Get-Content -LiteralPath 'C:\SandboxMediaApp\package-files.json' -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach($file in $taskFiles){Require ((Get-FileHash -LiteralPath ('C:\SandboxMediaApp\'+$file.path) -Algorithm SHA256).Hash -eq $file.sha256) ('Payload mismatch: '+$file.path)}
    foreach($sample in $taskManifest.samples){Require ((Get-FileHash -LiteralPath ($taskInput+'\'+$sample.file) -Algorithm SHA256).Hash -eq $sample.sha256) 'Sample hash mismatch'}
    $taskState.payload_files_verified=$taskFiles.Count;$taskState.exe_sha256=$taskManifest.exe_sha256;Save-Probe
    Run-Probe 'heic' $taskExe @('--decode-heic','C:\MediaProbeInput\samples\sample.heic','C:\MediaProbeOutput\decoded-heic.png')
    $taskState.heic=Get-Content -LiteralPath ($taskOutput+'\heic-stdout.log') -Raw | ConvertFrom-Json
    Require ($taskState.heic.width -gt 0 -and $taskState.heic.height -gt 0) 'HEIC decoded size missing'
    Save-Probe
    Run-Probe 'mov' $taskExe @('--probe-video','C:\MediaProbeInput\samples\sample.mov')
    $taskState.mov=Get-Content -LiteralPath ($taskOutput+'\mov-stdout.log') -Raw | ConvertFrom-Json
    Require ($taskState.mov.duration -gt 60 -and @($taskState.mov.seeks | Where-Object {-not $_.pass}).Count -eq 0) 'MOV null decode/seek failed'
    Save-Probe
    Require ((Get-FileHash -LiteralPath ($taskInput+'\opengl-probe.exe') -Algorithm SHA256).Hash -eq $taskManifest.opengl_probe_sha256) 'GL probe hash mismatch'
    try {
        Run-Probe 'opengl-capability' ($taskInput+'\opengl-probe.exe') @()
        $taskState.opengl=Get-Content -LiteralPath ($taskOutput+'\opengl-capability-stdout.log') -Raw | ConvertFrom-Json
    } catch {$taskState.opengl_probe_error=$_.Exception.Message}
    Save-Probe
    Require ((Get-FileHash -LiteralPath ($taskInput+'\installer.exe') -Algorithm SHA256).Hash -eq $taskManifest.installer_sha256) 'Installer hash mismatch'
    $taskInstall='C:\InstalledMediaApp';$taskRegistry='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\LargeScreenMediaBrowser.LocalTest'
    Require (-not (Test-Path -LiteralPath $taskInstall) -and -not (Test-Path -LiteralPath $taskRegistry)) 'Unexpected existing guest installation'
    Run-Probe 'install' ($taskInput+'\installer.exe') @('--silent','--install-dir',$taskInstall)
    $taskInstallManifest=Get-Content -LiteralPath ($taskInstall+'\installation.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    Require ($taskInstallManifest.registered -and (Test-Path -LiteralPath $taskRegistry)) 'Guest install registration missing'
    foreach($file in $taskInstallManifest.files){Require ((Get-FileHash -LiteralPath ($taskInstall+'\'+$file.path) -Algorithm SHA256).Hash -eq $file.sha256) ('Installed payload mismatch '+$file.path)}
    $taskState.installed_files_verified=$taskInstallManifest.files.Count
    Require ((Get-FileHash -LiteralPath ($taskInstall+'\media-browser.exe')).Hash -eq $taskManifest.exe_sha256) 'Installed application mismatch'
    Run-Probe 'installed-heic' ($taskInstall+'\media-browser.exe') @('--decode-heic','C:\MediaProbeInput\samples\sample.heic','C:\MediaProbeOutput\installed-heic.png')
    Run-Probe 'installed-mov' ($taskInstall+'\media-browser.exe') @('--probe-video','C:\MediaProbeInput\samples\sample.mov')
    $taskInstalledMov=Get-Content -LiteralPath ($taskOutput+'\installed-mov-stdout.log') -Raw | ConvertFrom-Json
    Require (@($taskInstalledMov.seeks | Where-Object {-not $_.pass}).Count -eq 0 -and $taskInstalledMov.duration -gt 60) 'Installed MOV null seek failed'
    $taskState.installed_heic_decode=$true;$taskState.installed_mov_null_seek=$true;Save-Probe
    Run-Probe 'uninstall' ($taskInstall+'\uninstall.exe') @('--silent','--uninstall')
    $taskCleanup=[Diagnostics.Stopwatch]::StartNew()
    while(((Test-Path -LiteralPath ($taskInstall+'\installation.json')) -or (Test-Path -LiteralPath ($taskInstall+'\uninstall.exe')) -or (Test-Path -LiteralPath $taskRegistry)) -and $taskCleanup.Elapsed.TotalSeconds -lt 30){Start-Sleep -Milliseconds 200}
    Require (-not (Test-Path -LiteralPath ($taskInstall+'\installation.json')) -and -not (Test-Path -LiteralPath ($taskInstall+'\uninstall.exe')) -and -not (Test-Path -LiteralPath $taskRegistry)) 'Guest uninstall incomplete'
    $taskState.install_uninstall_registry_verified=$true;Save-Probe
    New-Item -ItemType Directory -Path 'C:\GuestMediaProbe\png' | Out-Null
    Copy-Item -LiteralPath 'C:\MediaProbeInput\samples\sample.png' -Destination 'C:\GuestMediaProbe\png\sample.png'
    New-Item -ItemType Directory -Path ($taskOutput+'\png-ui') | Out-Null
    Run-Probe 'png-ui' $taskExe @('--format-smoke','C:\MediaProbeOutput\png-ui','--open','C:\GuestMediaProbe\png','--data-dir','C:\GuestMediaProbe\settings','--quit-after','40000')
    $taskReport=Get-Content -LiteralPath ($taskOutput+'\png-ui\report.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    Require ($null -eq $taskReport.error) 'Native PNG UI diagnostic failed'
    $taskPhases=@{};foreach($row in $taskReport.rows){$taskPhases[$row.phase]=$row}
    foreach($phase in @('list_thumbnail','detail_fit','detail_zoom','returned_list')){Require ($null -ne $taskPhases[$phase] -and $taskPhases[$phase].textures.Count -gt 0) ('Missing native phase '+$phase)}
    Add-Type -AssemblyName System.Drawing
    $taskBitmap=New-Object System.Drawing.Bitmap($taskOutput+'\png-ui\detail.png')
    try {
        $taskRow=$taskPhases['detail_fit'];$taskColors=@(@(220,25,35),@(25,200,55),@(35,55,220),@(225,210,25));$taskPoints=@(@(.2,.2),@(.8,.2),@(.2,.8),@(.8,.8));$taskPixels=@()
        for($i=0;$i -lt 4;$i++){$x=[int][Math]::Round(($taskRow.region[0]+($taskRow.region[2]-$taskRow.region[0])*$taskPoints[$i][0])*$taskRow.scale);$y=[int][Math]::Round(($taskRow.region[1]+($taskRow.region[3]-$taskRow.region[1])*$taskPoints[$i][1])*$taskRow.scale);$c=$taskBitmap.GetPixel($x,$y);$rgb=@([int]$c.R,[int]$c.G,[int]$c.B);$taskPixels+=,@($rgb);for($k=0;$k -lt 3;$k++){Require ([Math]::Abs($rgb[$k]-$taskColors[$i][$k]) -lt 32) 'PNG GPU quadrant mismatch'}}
        $taskState.native_png_corners=$taskPixels
    }finally{$taskBitmap.Dispose()}
    $taskState.native_png_gpu_verified=$true;$taskState.state='finished';$taskState.checks_pass=$true
}catch{$taskState.state='failed';$taskState.error=$_.Exception.ToString()}
finally{
    $taskState.ended_utc=[DateTime]::UtcNow.ToString('o');Save-Probe
    # Host harness closes only the launcher/client pair bound to this UUID config.
}
