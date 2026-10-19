Set-StrictMode -Version Latest

function Get-WalletVersion([string]$ProjectRoot) {
    $cargo = Get-Content -LiteralPath (Join-Path $ProjectRoot 'windows-source/Cargo.toml') -Raw
    $match = [regex]::Match($cargo, '(?m)^version = "(\d+\.\d+\.\d+)"\r?$')
    if (-not $match.Success) { throw 'No se encontró una versión estable de Cargo' }
    $version = $match.Groups[1].Value
    $lock = Get-Content -LiteralPath (Join-Path $ProjectRoot 'windows-source/Cargo.lock') -Raw
    if ($lock -notmatch ('name = "aircard"\r?\nversion = "' + [regex]::Escape($version) + '"')) { throw 'La versión raíz de Cargo.lock no coincide con Cargo.toml' }
    $resource = Get-Content -LiteralPath (Join-Path $ProjectRoot 'wallet-studio.rc') -Raw
    foreach ($field in @('FileVersion','ProductVersion')) {
        if (-not $resource.Contains('VALUE "' + $field + '", "' + $version + '\0"')) { throw "Versión incoherente en $field" }
    }
    foreach ($field in @('FILEVERSION','PRODUCTVERSION')) {
        if ($resource -notmatch ('(?m)^\s*' + $field + ' ' + $version.Replace('.', ',') + ',0\r?$')) { throw "Versión incoherente en $field" }
    }
    return $version
}

function Get-SafeDistributionFile([string]$ProjectRoot, [string]$Relative) {
    if ($Relative -notmatch '^[a-zA-Z0-9_ .+()/\-]+$' -or $Relative.Contains('\') -or
        $Relative.StartsWith('/') -or @($Relative.Split('/') | Where-Object { $_ -in @('', '.', '..') }).Count) { throw "Ruta no permitida: $Relative" }
    $cursor = [IO.Path]::GetFullPath($ProjectRoot)
    if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'La raíz es un enlace' }
    foreach ($part in $Relative.Split('/')) {
        $cursor = Join-Path $cursor $part
        $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "No se permiten enlaces: $Relative" }
    }
    if ($item.PSIsContainer) { throw "Se esperaba un archivo: $Relative" }
    return $item.FullName
}

function Get-LicenseGaps([string]$ProjectRoot) {
    $index = Get-Content -LiteralPath (Join-Path $ProjectRoot 'third-party-licenses/INDEX.json') -Raw | ConvertFrom-Json
    $lock = Get-Content -LiteralPath (Join-Path $ProjectRoot 'windows-source/Cargo.lock') -Raw
    foreach ($block in ($lock -split '\[\[package\]\]')) {
        if ($block -notmatch '(?m)^source = ') { continue }
        $name = [regex]::Match($block, '(?m)^name = "([^"]+)"').Groups[1].Value
        $version = [regex]::Match($block, '(?m)^version = "([^"]+)"').Groups[1].Value
        $entries = @($index.crates | Where-Object { $_.crate -eq $name -and $_.version -eq $version })
        if ($entries.Count -ne 1 -or @($entries[0].includedNotices).Count -eq 0) { "$name $version"; continue }
        foreach ($notice in $entries[0].includedNotices) { $null = Get-SafeDistributionFile $ProjectRoot "third-party-licenses/$name-$version/$notice" }
    }
}

function Get-DistributionPlan([string]$ProjectRoot) {
    $manifest = Get-Content -LiteralPath (Join-Path $ProjectRoot 'distribution/files.json') -Raw | ConvertFrom-Json
    foreach ($entry in $manifest.files) {
        if ($entry.destination -notmatch '^[a-zA-Z0-9_ .-]+$' -or $entry.destination -in @('.', '..')) { throw 'Destino de paquete no válido' }
        [pscustomobject]@{ Source = (Get-SafeDistributionFile $ProjectRoot $entry.source); Relative = $entry.destination }
    }
    foreach ($relative in @('third-party-licenses/INDEX.json', 'third-party-licenses/LICENSE-Windows-handshake.txt')) {
        [pscustomobject]@{ Source = (Get-SafeDistributionFile $ProjectRoot $relative); Relative = $relative }
    }
    $index = Get-Content -LiteralPath (Join-Path $ProjectRoot 'third-party-licenses/INDEX.json') -Raw | ConvertFrom-Json
    foreach ($crate in $index.crates) {
        foreach ($notice in $crate.includedNotices) {
            $relative = "third-party-licenses/$($crate.crate)-$($crate.version)/$notice"
            [pscustomobject]@{ Source = (Get-SafeDistributionFile $ProjectRoot $relative); Relative = $relative }
        }
    }
    [pscustomobject]@{ Source = (Get-SafeDistributionFile $ProjectRoot 'windows/designs/README.md'); Relative = 'designs/README.md' }
    $designs = Get-Content -LiteralPath (Join-Path $ProjectRoot 'distribution/designs.json') -Raw | ConvertFrom-Json
    foreach ($design in $designs.designs) {
        if ($design.file -notmatch '^[a-zA-Z0-9_-]+\.(png|jpg|jpeg|webp|gif)$' -or $design.file.StartsWith('guardado-')) { throw 'Nombre de diseño no autorizado' }
        if ($design.sha256 -notmatch '^[a-fA-F0-9]{64}$' -or [string]::IsNullOrWhiteSpace($design.rights)) { throw 'Faltan hash o derechos del diseño' }
        $source = Get-SafeDistributionFile $ProjectRoot ('windows/designs/' + $design.file)
        if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $design.sha256) { throw "El diseño autorizado cambió: $($design.file)" }
        [pscustomobject]@{ Source = $source; Relative = ('designs/' + $design.file) }
        if ($design.licenseFile -notmatch '^distribution/design-licenses/[a-zA-Z0-9_.-]+\.(txt|md)$') { throw 'Aviso de diseño fuera de distribution/design-licenses' }
        $license = Get-SafeDistributionFile $ProjectRoot $design.licenseFile
        [pscustomobject]@{ Source = $license; Relative = ('designs/' + [IO.Path]::GetFileName($license)) }
    }
}
