param([switch]$TestOnly)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
$walletPreviousRustFlags = $env:RUSTFLAGS
$walletPreviousEncodedRustFlags = $env:CARGO_ENCODED_RUSTFLAGS
try {
    # Cargo applies these flags to the entire target dependency graph, including
    # native dependency build scripts, not just the final rustc invocation.
    if ($env:CARGO_ENCODED_RUSTFLAGS) {
        $env:CARGO_ENCODED_RUSTFLAGS += ([char]31 + '-C' + [char]31 + 'target-feature=+crt-static')
    } else { $env:RUSTFLAGS = ($env:RUSTFLAGS + ' -C target-feature=+crt-static').Trim() }
    if ($TestOnly) {
        cargo test --release --locked --manifest-path windows-source/Cargo.toml -- --test-threads=1 --skip test_usbmux_query --skip test_list_connected_devices --skip test_syslog_service_receive --skip test_saved_cards_purging
        if ($LASTEXITCODE -ne 0) { throw 'Las pruebas fallaron' }
        return
    }
    if (-not (Get-Command rc.exe -ErrorAction SilentlyContinue)) {
        throw 'Abre una consola de desarrollo MSVC con Windows SDK para disponer de rc.exe.'
    }
    $resourcePath = Join-Path $env:TEMP ('wallet-studio-' + [Guid]::NewGuid().ToString('N') + '.res')
    rc.exe /nologo /fo $resourcePath wallet-studio.rc
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo compilar el recurso Windows' }
    cargo rustc --release --locked --manifest-path windows-source/Cargo.toml --bin aircard -- -C "link-arg=$resourcePath"
    if ($LASTEXITCODE -ne 0) { throw 'La compilación falló' }
    # The portable package does not redistribute the MSVC runtime or Apple DLLs.
    $walletImports = & dumpbin.exe /nologo /dependents 'windows-source/target/release/aircard.exe'
    if ($LASTEXITCODE -ne 0) { throw 'No se pudieron comprobar las dependencias Windows' }
    if ($walletImports -match '(?i)(?:VCRUNTIME|MSVCP|CONCRT)[^\s]*\.dll') { throw 'El ejecutable requiere DLL externas del runtime MSVC' }
    Copy-Item -LiteralPath 'windows-source/target/release/aircard.exe'  -Destination 'windows/Wallet Studio.exe' -Force
} finally { $env:RUSTFLAGS = $walletPreviousRustFlags; $env:CARGO_ENCODED_RUSTFLAGS = $walletPreviousEncodedRustFlags; Pop-Location }

