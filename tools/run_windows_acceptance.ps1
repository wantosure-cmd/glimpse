param(
    [Parameter(Mandatory=$true)][string]$PortableZip,
    [string[]]$CaseName=@(),
    [string]$OutputRoot=''
)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.Drawing
$taskKit=(Resolve-Path -LiteralPath $PSScriptRoot).Path
$taskManifest=Get-Content -LiteralPath (Join-Path $taskKit 'manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$taskArchive=(Resolve-Path -LiteralPath $PortableZip).Path
function Get-Sha([string]$Path) {return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
function Require([bool]$Condition,[string]$Message) {if(-not $Condition){throw $Message}}
function Child-Path([string]$Base,[string]$Relative) {
    Require (-not [IO.Path]::IsPathRooted($Relative)) 'Absolute payload path refused'
    $path=[IO.Path]::GetFullPath((Join-Path $Base $Relative))
    Require ($path.StartsWith($Base.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) 'Payload escapes its owned directory'
    return $path
}
Require ((Get-Sha $taskArchive) -eq $taskManifest.portable_sha256) 'Portable ZIP hash does not match this acceptance kit'
Require ((Get-Sha (Join-Path $taskKit 'audio-meter.exe')) -eq $taskManifest.meter_sha256) 'Audio meter hash mismatch'
if($OutputRoot){$taskOut=[IO.Path]::GetFullPath($OutputRoot)}else{$taskOut=Join-Path $taskKit ('results-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmss')+'-'+[Guid]::NewGuid().ToString('N'))}
Require (-not (Test-Path -LiteralPath $taskOut)) 'Output directory must be new; existing directories are never reused'
New-Item -ItemType Directory -Path $taskOut | Out-Null
Set-Content -LiteralPath (Join-Path $taskOut '.acceptance-owned') -Value 'Generated offline acceptance output' -Encoding ASCII
$taskUnpack=Join-Path $taskOut 'application';New-Item -ItemType Directory -Path $taskUnpack | Out-Null
$taskZip=[IO.Compression.ZipFile]::OpenRead($taskArchive)
try {foreach($entry in $taskZip.Entries){$null=Child-Path $taskUnpack $entry.FullName}}finally{$taskZip.Dispose()}
[IO.Compression.ZipFile]::ExtractToDirectory($taskArchive,$taskUnpack)
$taskAppRoot=Join-Path $taskUnpack 'media-browser-portable'
$taskExe=Join-Path $taskAppRoot 'media-browser.exe'
Require ((Get-Sha $taskExe) -eq $taskManifest.exe_sha256) 'Extracted application hash mismatch'
$taskFiles=Get-Content -LiteralPath (Join-Path $taskAppRoot 'package-files.json') -Raw -Encoding UTF8 | ConvertFrom-Json
foreach($file in $taskFiles){Require ((Get-Sha (Child-Path $taskAppRoot $file.path)) -eq $file.sha256) ('Runtime payload mismatch: '+$file.path)}
$taskCases=@($taskManifest.cases | Where-Object {$CaseName.Count -eq 0 -or $_.name -in $CaseName})
Require ($taskCases.Count -gt 0) 'No selected cases'
foreach($name in $CaseName){Require ($name -in @($taskManifest.cases.name)) ('Unknown case: '+$name)}
$taskHostWarnings=@()
try{$taskOS=Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber}catch{$taskOS=$null;$taskHostWarnings+='CIM OS query unavailable: '+$_.Exception.Message}
$taskRegistry=Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' | Select-Object ProductName,CurrentBuildNumber,UBR,DisplayVersion
$taskRows=New-Object 'System.Collections.Generic.List[object]'
$taskLiveProcess=$false
function Read-Pixels([string]$File,$Row){
    Require ($null -ne $Row.region) 'Missing actually drawn image/video region'
    $bitmap=New-Object System.Drawing.Bitmap($File)
    try{
        $points=New-Object 'System.Collections.Generic.List[object]'
        for($y=1;$y -lt 12;$y++){for($x=1;$x -lt 12;$x++){
            $px=[int][Math]::Round(($Row.region[0]+($Row.region[2]-$Row.region[0])*$x/12.0)*$Row.scale)
            $py=[int][Math]::Round(($Row.region[1]+($Row.region[3]-$Row.region[1])*$y/12.0)*$Row.scale)
            Require ($px -ge 0 -and $px -lt $bitmap.Width -and $py -ge 0 -and $py -lt $bitmap.Height) 'Drawn region is outside screenshot'
            $c=$bitmap.GetPixel($px,$py);$points.Add(@([int]$c.R,[int]$c.G,[int]$c.B))
        }}
        $corners=New-Object 'System.Collections.Generic.List[object]'
        foreach($point in @(@(0.2,0.2),@(0.8,0.2),@(0.2,0.8),@(0.8,0.8))){
            $px=[int][Math]::Round(($Row.region[0]+($Row.region[2]-$Row.region[0])*$point[0])*$Row.scale)
            $py=[int][Math]::Round(($Row.region[1]+($Row.region[3]-$Row.region[1])*$point[1])*$Row.scale)
            $c=$bitmap.GetPixel($px,$py);$corners.Add(@([int]$c.R,[int]$c.G,[int]$c.B))
        }
        return @{samples=$points.ToArray();corners=$corners.ToArray();screenshot_sha256=(Get-Sha $File)}
    }finally{$bitmap.Dispose()}
}
function Mean-Difference($A,$B){$sum=0.0;for($i=0;$i -lt $A.Count;$i++){for($c=0;$c -lt 3;$c++){$sum+=[Math]::Abs($A[$i][$c]-$B[$i][$c])}};return $sum/($A.Count*3)}
function Save-Result([string]$State){
    $pass=@($taskRows | Where-Object {-not $_.passed}).Count -eq 0 -and $taskRows.Count -eq $taskCases.Count
    $complete=$taskCases.Count -eq $taskManifest.cases.Count -and $taskRows.Count -eq $taskCases.Count
    $result=[ordered]@{state=$State;checks_pass=($State -eq 'finished' -and $pass -and $complete);selected_cases_pass=($State -eq 'finished' -and $pass);full_suite_completed=$complete;exe_sha256=$taskManifest.exe_sha256;portable_sha256=$taskManifest.portable_sha256;os=$taskOS;registry_os=$taskRegistry;host_warnings=$taskHostWarnings;powershell=$PSVersionTable.PSVersion.ToString();logical_processors=[Environment]::ProcessorCount;output=$taskOut;rows=$taskRows.ToArray();scope='Actual native sample decoding, FBO pixels, audio session and seek; no physical-input, clean-system provenance, HDR calibration or whole-product acceptance claim'}
    $result | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $taskOut 'result.json') -Encoding UTF8
}
Save-Result 'running'
foreach($case in $taskCases){
    $evidence=Join-Path $taskOut $case.name;New-Item -ItemType Directory -Path $evidence | Out-Null
    $row=[ordered]@{name=$case.name;passed=$false;evidence=$evidence;started_utc=[DateTime]::UtcNow.ToString('o')}
    $app=$null;$meter=$null
    try{
        $source=Child-Path $taskKit $case.file
        Require ((Get-Sha $source) -eq $case.sha256) 'Sample hash mismatch before browsing'
        $data=Join-Path $evidence 'data';$sourceDir=Split-Path $source -Parent
        $app=Start-Process -FilePath $taskExe -ArgumentList '--format-smoke',('"'+$evidence+'"'),'--open',('"'+$sourceDir+'"'),'--data-dir',('"'+$data+'"'),'--quit-after','40000' -WorkingDirectory $taskOut -RedirectStandardOutput (Join-Path $evidence 'stdout.log') -RedirectStandardError (Join-Path $evidence 'stderr.log') -PassThru
        $null=$app.Handle
        $row['pid']=$app.Id
        $meter=$null
        if($case.kind -eq 'video'){$meter=Start-Process -FilePath (Join-Path $taskKit 'audio-meter.exe') -ArgumentList $app.Id -WindowStyle Hidden -RedirectStandardOutput (Join-Path $evidence 'audio.json') -RedirectStandardError (Join-Path $evidence 'audio.stderr.log') -PassThru;$null=$meter.Handle}
        Require ($app.WaitForExit(60000)) ('Diagnostic is still live; inspect its PID before retrying: '+$app.Id);$app.WaitForExit();$row['exit_code']=$app.ExitCode;Require ($app.ExitCode -eq 0) 'Application exit was not successful'
        if($meter){Require ($meter.WaitForExit(25000)) ('Audio diagnostic is still live, PID '+$meter.Id);$meter.WaitForExit();Require ($meter.ExitCode -eq 0) 'Audio endpoint/session initialization failed'}
        $report=Get-Content -LiteralPath (Join-Path $evidence 'report.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        Require ($null -eq $report.error) ('Native diagnostic error: '+$report.error)
        $phases=@{};foreach($phase in $report.rows){$phases[$phase.phase]=$phase;Require ($phase.failures.Count -eq 0) 'Native thumbnail failure'}
        $listed=$phases['list_thumbnail'];$detail=$phases['detail_fit']
        Require ($listed.record.width -eq $case.width -and $listed.record.height -eq $case.height) 'Display size/rotation/SAR mismatch'
        Require ($listed.textures.Count -gt 0 -and $phases['returned_list'].textures.Count -gt 0) 'No real list texture'
        $first=Read-Pixels (Join-Path $evidence 'list.png') $listed;$second=Read-Pixels (Join-Path $evidence 'detail.png') $detail
        if($case.kind -eq 'image'){
            $zoom=$phases['detail_zoom'];Require ($zoom.zoom -gt 0 -and $zoom.textures.Count -gt 0 -and $null -ne $zoom.region) 'Full-resolution image request failed'
            $max=0;foreach($texture in $zoom.textures){$max=[Math]::Max($max,[Math]::Max($texture[0],$texture[1]))};Require ($max -ge [Math]::Max($case.width,$case.height)) 'Original-sized image texture missing'
            if($case.PSObject.Properties['corners']){foreach($image in @($first,$second)){for($i=0;$i -lt 4;$i++){for($c=0;$c -lt 3;$c++){Require ([Math]::Abs($image.corners[$i][$c]-$case.corners[$i][$c]) -lt 32) 'Known quadrant orientation mismatch'}}};$row['quadrants_verified']=$true}
        }else{
            $moving=$phases['playing_advanced'];$seek=$phases['paused_seek'];$player=$detail.player
            Require ($player.ready -and $player.mute -eq 'no' -and $player.vo -eq 'libmpv') 'Player is not ready/audible/embedded'
            Require ($player.duration -gt $case.expected.duration_min -and $moving.player.time -gt $player.time+0.1) 'Video did not advance'
            Require ($seek.player.pause -eq 'yes' -and [Math]::Abs($seek.player.time-$seek.seek_target) -lt 0.2 -and $seek.seek_target -gt 29) 'Paused +30s seek failed'
            $after=Read-Pixels (Join-Path $evidence 'after.png') $seek;$delta=Mean-Difference $second.samples $after.samples;Require ($delta -gt 1) 'Seek produced no new visible image'
            $stable=0;for($i=0;$i -lt $first.samples.Count;$i++){$distance=0;for($c=0;$c -lt 3;$c++){$distance+=[Math]::Abs($first.samples[$i][$c]-$second.samples[$i][$c])};if($distance/3 -lt 50){$stable++}};Require ($stable/$first.samples.Count -gt 0.6) 'Poster/detail spatial orientation mismatch'
            $audio=Get-Content -LiteralPath (Join-Path $evidence 'audio.json') -Raw -Encoding UTF8 | ConvertFrom-Json
            $peak=($audio | Where-Object {$_.pid -eq $app.Id} | Measure-Object peak -Maximum).Maximum;Require ($peak -gt 0.0001) 'No per-process audio signal'
            $row['audio_peak']=$peak;$row['visible_seek_pixel_difference']=$delta;$row['poster_detail_matching_samples']=$stable
        }
        Require ((Get-Sha $source) -eq $case.sha256) 'Sample was changed by browsing'
        $row['source_unchanged']=$true;$row['list_pixels']=$first;$row['detail_pixels']=$second;$row['passed']=$true
        Write-Host ('PASS '+$case.name)
    }catch{
        $row['failure']=$_.Exception.Message;Write-Host ('FAIL '+$case.name+': '+$_.Exception.Message)
        if($app -and -not $app.HasExited){$taskLiveProcess=$true;$row['live_app_pid']=$app.Id}
    }finally{
        if($meter -and -not $meter.HasExited){if(-not $meter.WaitForExit(25000)){$taskLiveProcess=$true;$row['live_audio_pid']=$meter.Id}}
    }
    $row['ended_utc']=[DateTime]::UtcNow.ToString('o');$taskRows.Add($row);Save-Result 'running'
    if($taskLiveProcess){Save-Result 'stopped_live_process';Write-Host 'Owned diagnostic still live. Inspect the recorded PID; no duplicate test was started.';exit 2}
}
Save-Result 'finished'
Write-Host ('Report: '+(Join-Path $taskOut 'result.json'))
if(@($taskRows | Where-Object {-not $_.passed}).Count -gt 0){exit 1}
