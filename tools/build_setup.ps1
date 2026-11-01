$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskOldFlags=$env:RUSTFLAGS
try{
    # All Rust crates, not only the final binary, use the static MSVC CRT.
    $env:RUSTFLAGS=($taskOldFlags+' -C target-feature=+crt-static').Trim()
    Push-Location -LiteralPath $taskRoot
    try{
        & cargo rustc --release --locked --offline --bin media-browser-setup --target-dir target/setup-static -- -C 'link-arg=/MANIFEST:EMBED' -C ('link-arg=/MANIFESTINPUT:'+(Join-Path $taskRoot 'assets/setup.manifest'))
        if($LASTEXITCODE -ne 0){throw 'Static CRT installer build failed'}
    }finally{Pop-Location}
}finally{$env:RUSTFLAGS=$taskOldFlags}
