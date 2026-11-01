$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$dist=Join-Path $taskRoot 'dist/browser-preview'
New-Item -ItemType Directory -Force -Path "$dist/runtime" | Out-Null
Copy-Item -LiteralPath "$taskRoot/target/release/media-browser.exe" -Destination $dist
Copy-Item -Path "$taskRoot/vendor/runtime/*" -Destination "$dist/runtime"
# Windows must resolve the application's imported CRT before Rust can configure
# its restricted native DLL search. The loader does not search runtime/ here.
Copy-Item -LiteralPath "$taskRoot/vendor/runtime/vcruntime140.dll" -Destination "$dist/vcruntime140.dll"
Copy-Item -LiteralPath "$taskRoot/assets/NotoSansSC-OFL.txt" -Destination "$dist/runtime/NotoSansSC-OFL.txt"
foreach($taskLicense in @(
    @('vendor/extracted/mpv/LICENSE.GPL','libmpv-LICENSE.GPL'),
    @('vendor/extracted/sources/libheif-1.23.6/COPYING','libheif-COPYING'),
    @('vendor/extracted/sources/libde265-1.1.3/COPYING','libde265-COPYING')
)){
    $taskSource=Join-Path $taskRoot $taskLicense[0]
    $taskDestination=Join-Path "$dist/runtime" $taskLicense[1]
    if(Test-Path -LiteralPath $taskSource){Copy-Item -LiteralPath $taskSource -Destination $taskDestination}
    elseif(-not (Test-Path -LiteralPath $taskDestination)){throw "Missing runtime license: $($taskLicense[1])"}
}
Copy-Item -Path "$taskRoot/assets/licenses/jpeg-decoder-*.txt" -Destination "$dist/runtime"
Write-Output $dist
