param([string]$Version, [switch]$Draft, [switch]$RequireSigned)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'distribution-common.ps1')
$projectRoot = Split-Path -Parent $PSScriptRoot
$actualVersion = Get-WalletVersion $projectRoot
if ($Version -and $Version -ne $actualVersion) { throw 'La versión solicitada no coincide con Cargo y los recursos Windows' }
$Version = $actualVersion
$gaps = @(Get-LicenseGaps $projectRoot)
if ($gaps.Count -and -not $Draft) { throw "Faltan avisos de $($gaps.Count) dependencias. No se puede distribuir. Consulta PREPARACION-COMERCIAL.md." }
$binary = Get-SafeDistributionFile $projectRoot 'windows/Wallet Studio.exe'
if ((Get-Item -LiteralPath $binary).VersionInfo.FileVersion -notmatch ('^' + [regex]::Escape($Version) + '(?:\.0)?$')) { throw 'La versión del ejecutable no coincide' }
$signature = Get-AuthenticodeSignature -LiteralPath $binary
if (($RequireSigned -or -not $Draft) -and $signature.Status -ne 'Valid') { throw 'La entrega comercial requiere Authenticode válido; -Draft solo permite revisión interna' }
$plan = @(Get-DistributionPlan $projectRoot)
$unique = @{}
foreach ($entry in $plan) {
    if ($unique.ContainsKey($entry.Relative)) {
        if ($unique[$entry.Relative].Source -ne $entry.Source) { throw 'Dos archivos comparten destino' }
    } else { $unique[$entry.Relative] = $entry }
}
$plan = @($unique.Values | Sort-Object Relative)
$outputRoot = Join-Path $projectRoot 'dist'
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
if ((Get-Item -LiteralPath $outputRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'dist no puede ser un enlace' }
$buildDir = Join-Path $outputRoot ((Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N'))
$label = if ($Draft) { 'BORRADOR-NO-VENDER-' } else { '' }
$leaf = "${label}Wallet-Studio-$Version-Windows"
$package = Join-Path $buildDir $leaf
New-Item -ItemType Directory -Path $package -Force | Out-Null
$checksums = @()
foreach ($entry in $plan) {
    $destination = Join-Path $package $entry.Relative
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    Copy-Item -LiteralPath $entry.Source -Destination $destination
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne (Get-FileHash -LiteralPath $entry.Source -Algorithm SHA256).Hash) { throw 'La copia no coincide con el origen' }
    $checksums += "$hash  $($entry.Relative)"
}
$versionInfo = [ordered]@{ product='Wallet Studio'; version=$Version; platform='windows-x64'; draft=[bool]$Draft; missingLicenseNotices=$gaps; activationEnforced=$false; automaticUpdates=$false; bundledDesignCount=@((Get-Content (Join-Path $projectRoot 'distribution/designs.json') -Raw | ConvertFrom-Json).designs).Count }
$versionInfo | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $package 'VERSION.json') -Encoding utf8
$checksums += ((Get-FileHash -LiteralPath (Join-Path $package 'VERSION.json') -Algorithm SHA256).Hash.ToLowerInvariant() + '  VERSION.json')
$checksums | Set-Content -LiteralPath (Join-Path $package 'SHA256SUMS.txt') -Encoding ascii
$zipPath = Join-Path $buildDir ($leaf + '.zip')
Compress-Archive -LiteralPath $package -DestinationPath $zipPath
# Reabrir el ZIP y verificar cada archivo contra las copias preparadas, sin extraer rutas.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($zipPath)
try {
    $expected = @{}
    foreach ($file in Get-ChildItem -LiteralPath $package -Recurse -File) {
        $relative = $file.FullName.Substring($package.Length + 1).Replace('\','/')
        $expected["$leaf/$relative"] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
    }
    $seen = @{}
    foreach ($entry in $archive.Entries) {
        $name = $entry.FullName.Replace('\','/')
        if ($name.EndsWith('/')) { continue }
        if (-not $expected.ContainsKey($name) -or $seen.ContainsKey($name)) { throw "Entrada inesperada o duplicada en ZIP: $name" }
        $stream = $entry.Open(); $sha = [Security.Cryptography.SHA256]::Create()
        try { $hash = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-','') } finally { $sha.Dispose(); $stream.Dispose() }
        if ($hash -ne $expected[$name]) { throw "Hash incorrecto dentro del ZIP: $name" }
        $seen[$name] = $true
    }
    if ($seen.Count -ne $expected.Count) { throw 'ZIP incompleto' }
} finally { $archive.Dispose() }
$hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$thumbprint = if ($signature.SignerCertificate) { $signature.SignerCertificate.Thumbprint } else { $null }
$manifest = [ordered]@{ version=$Version; platform='windows-x64'; asset=(Split-Path -Leaf $zipPath); sha256=$hash; size=(Get-Item $zipPath).Length; authenticodeStatus=$signature.Status.ToString(); signerThumbprint=$thumbprint; draft=[bool]$Draft; updateMode='manual-authorized-customers'; activationEnforced=$false; missingLicenseNotices=$gaps; animatedWalletSupport='not-established' }
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $buildDir 'release-manifest.json') -Encoding utf8
($hash + '  ' + (Split-Path -Leaf $zipPath)) | Set-Content -LiteralPath (Join-Path $buildDir 'SHA256SUMS.txt') -Encoding ascii
Write-Output $zipPath
Write-Output (Join-Path $buildDir 'release-manifest.json')
if ($Draft) { Write-Warning 'BORRADOR: no subir a Shopify ni vender. Revisar licencias, firma, activación y validación nativa.' }
