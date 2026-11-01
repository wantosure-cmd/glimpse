$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path $PSScriptRoot -Parent
$cmake = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe'
if (!(Test-Path -LiteralPath $cmake)) { $cmake = (Get-Command cmake).Source }
$prefix = Join-Path $taskRoot 'vendor/native'
$sourceRoot = Join-Path $taskRoot 'vendor/extracted/sources'
& $cmake -S "$sourceRoot/libde265-1.1.3" -B "$taskRoot/target/native/de265" -G 'Visual Studio 17 2022' -A x64 "-DCMAKE_INSTALL_PREFIX=$prefix" '-DCMAKE_CXX_FLAGS=/utf-8 /MP4' -DENABLE_DECODER=OFF -DENABLE_ENCODER=OFF -DENABLE_SDL=OFF -DBUILD_SHARED_LIBS=ON
if ($LASTEXITCODE) { throw 'libde265 configure failed' }
& $cmake --build "$taskRoot/target/native/de265" --config Release --target INSTALL -j 4
if ($LASTEXITCODE) { throw 'libde265 build failed' }
& $cmake -S "$sourceRoot/libheif-1.23.6" -B "$taskRoot/target/native/heif" -G 'Visual Studio 17 2022' -A x64 "-DCMAKE_INSTALL_PREFIX=$prefix" "-DCMAKE_PREFIX_PATH=$prefix" '-DCMAKE_CXX_FLAGS=/utf-8 /MP4' -DWITH_LIBDE265=ON -DWITH_LIBDE265_PLUGIN=OFF -DENABLE_PLUGIN_LOADING=OFF -DWITH_X265=OFF -DWITH_X264=OFF -DWITH_OpenH264_DECODER=OFF -DWITH_AOM_DECODER=OFF -DWITH_AOM_ENCODER=OFF -DWITH_LIBSHARPYUV=OFF -DWITH_EXAMPLES=OFF -DWITH_GDK_PIXBUF=OFF -DBUILD_TESTING=OFF -DBUILD_DOCUMENTATION=OFF -DBUILD_SHARED_LIBS=ON
if ($LASTEXITCODE) { throw 'libheif configure failed' }
& $cmake --build "$taskRoot/target/native/heif" --config Release --target INSTALL -j 4
if ($LASTEXITCODE) { throw 'libheif build failed' }
