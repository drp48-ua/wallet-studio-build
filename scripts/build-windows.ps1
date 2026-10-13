param([switch]$TestOnly)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    if ($TestOnly) {
        cargo test --release --locked --manifest-path windows-source/Cargo.toml -- --skip test_usbmux_query --skip test_list_connected_devices --skip test_syslog_service_receive --skip test_saved_cards_purging
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
    Copy-Item -LiteralPath 'windows-source/target/release/aircard.exe' -Destination 'windows/Wallet Studio.exe' -Force
} finally { Pop-Location }

