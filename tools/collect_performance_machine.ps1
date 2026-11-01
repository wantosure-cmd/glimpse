param([string]$Prefix='deletefinal')
$ErrorActionPreference='Stop'
if($Prefix -notmatch '^[a-zA-Z0-9_-]+$'){throw 'Invalid prefix'}
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskCpu=Get-ItemProperty 'HKLM:/HARDWARE/DESCRIPTION/System/CentralProcessor/0'
$taskOs=Get-ItemProperty 'HKLM:/SOFTWARE/Microsoft/Windows NT/CurrentVersion'
$taskGraphics=Get-ChildItem 'HKLM:/SYSTEM/CurrentControlSet/Control/Class/{4d36e968-e325-11ce-bfc1-08002be10318}' -ErrorAction SilentlyContinue | ForEach-Object {Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue} | Where-Object DriverDesc | Select-Object DriverDesc,DriverVersion
$taskDrive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($taskRoot))
if(-not ('MediaBrowserInstalledMemoryProbe' -as [type])){Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public static class MediaBrowserInstalledMemoryProbe{[DllImport("kernel32.dll",SetLastError=true)][return:MarshalAs(UnmanagedType.Bool)]public static extern bool GetPhysicallyInstalledSystemMemory(out ulong kb);}'}
[uint64]$taskMemoryKb=0;$taskMemoryOk=[MediaBrowserInstalledMemoryProbe]::GetPhysicallyInstalledSystemMemory([ref]$taskMemoryKb)
[pscustomobject]@{
 recorded_utc=[DateTime]::UtcNow;cpu_name=$taskCpu.ProcessorNameString;logical_processors=[Environment]::ProcessorCount
 os=[pscustomobject]@{product=$taskOs.ProductName;display_version=$taskOs.DisplayVersion;build=$taskOs.CurrentBuildNumber;ubr=$taskOs.UBR}
 registered_display_adapters=$taskGraphics
 storage=[pscustomobject]@{drive=$taskDrive.Name;format=$taskDrive.DriveFormat;total_bytes=$taskDrive.TotalSize}
 installed_memory_bytes=$(if($taskMemoryOk){$taskMemoryKb*1024}else{$null})
 physical_memory_probe=[pscustomobject]@{api='GetPhysicallyInstalledSystemMemory';success=$taskMemoryOk;installed_kilobytes=$taskMemoryKb}
 wmi_query='Not repeated; same-thread WMI attempt previously returned access denied'
 active_gpu_and_physical_ssd='Not independently determined for application; separate WGL capability probe recorded'
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $taskRoot "artifacts/$Prefix-performance-machine.json") -Encoding utf8
& (Join-Path $taskRoot 'target/diagnostics/Release/opengl-probe.exe') | Set-Content (Join-Path $taskRoot "artifacts/$Prefix-opengl-capability.json") -Encoding utf8
