$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path $PSScriptRoot -Parent
$runtime = Join-Path $taskRoot 'vendor/runtime'
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
Copy-Item -LiteralPath "$taskRoot/vendor/extracted/mpv/libmpv-2.dll" -Destination $runtime
Copy-Item -Path "$taskRoot/vendor/native/bin/*.dll" -Destination $runtime
# App-local MSVC runtime for the source-built libraries; no global installer required.
$redist = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Redist\MSVC'
$crt = Get-ChildItem -LiteralPath $redist -Directory | Sort-Object Name -Descending | ForEach-Object { Join-Path $_.FullName 'x64/Microsoft.VC143.CRT' } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($crt) { Copy-Item -Path "$crt/*.dll" -Destination $runtime }
$dist = Join-Path $taskRoot 'dist/p0-portable'
New-Item -ItemType Directory -Force -Path "$dist/runtime" | Out-Null
Copy-Item -LiteralPath "$taskRoot/target/release/media-browser.exe" -Destination $dist
Copy-Item -Path "$runtime/*.dll" -Destination "$dist/runtime"
Copy-Item -LiteralPath "$taskRoot/vendor/extracted/mpv/LICENSE.GPL" -Destination "$dist/runtime/libmpv-LICENSE.GPL"
Copy-Item -LiteralPath "$taskRoot/vendor/extracted/sources/libheif-1.23.6/COPYING" -Destination "$dist/runtime/libheif-COPYING"
Copy-Item -LiteralPath "$taskRoot/vendor/extracted/sources/libde265-1.1.3/COPYING" -Destination "$dist/runtime/libde265-COPYING"
Copy-Item -LiteralPath "$taskRoot/assets/NotoSansSC-OFL.txt" -Destination "$dist/runtime/NotoSansSC-OFL.txt"
Get-ChildItem -LiteralPath "$dist/runtime" -File | ForEach-Object {
    [pscustomobject]@{ file=$_.Name; bytes=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
} | ConvertTo-Json | Set-Content "$taskRoot/vendor/runtime-manifest.json" -Encoding utf8
Write-Output $dist
