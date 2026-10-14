param([string]$Version = '0.6.0', [switch]$RequireSigned)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'VersiÃƒÂ³n no vÃƒÂ¡lida' }
$projectRoot = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $projectRoot 'windows/Wallet Studio.exe'
if ((Get-Item -LiteralPath $binary).VersionInfo.FileVersion -ne $Version) { throw 'La versiÃƒÂ³n del ejecutable no coincide' }
$signature = Get-AuthenticodeSignature -LiteralPath $binary
if ($RequireSigned -and $signature.Status -ne 'Valid') { throw 'La distribuciÃƒÂ³n requiere una firma Authenticode vÃƒÂ¡lida' }
$outputRoot = Join-Path $projectRoot 'dist'
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
$staging = Join-Path $outputRoot ('stage-' + [Guid]::NewGuid().ToString('N'))
$package = Join-Path $staging ('Wallet-Studio-' + $Version)
New-Item -ItemType Directory -Path (Join-Path $package 'designs') -Force | Out-Null
foreach ($relative in @('windows/Wallet Studio.exe', 'windows/ABRIR-WALLET-STUDIO.cmd', 'windows/LICENSE-AirCard-Windows.txt')) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $relative) -Destination $package
}
foreach ($relative in @('LICENSE','LEEME.md','ACTUALIZACIONES.md','SEGURIDAD.md','ANIMACION-Y-WALLET.md','PRUEBA-WINDOWS.md',('NOVEDADES-' + $Version + '.md'))) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $relative) -Destination $package
}
Copy-Item -LiteralPath (Join-Path $projectRoot 'third-party-licenses') -Destination $package -Recurse
$designs = Get-ChildItem -LiteralPath (Join-Path $projectRoot 'windows/designs') -File
foreach ($design in $designs) {
    if ($design.Name -match 'fanart' -or $design.LinkType) { continue }
    if ($design.Extension.ToLowerInvariant() -notin @('.png','.jpg','.jpeg','.webp','.gif','.md','.json')) { continue }
    Copy-Item -LiteralPath $design.FullName -Destination (Join-Path $package 'designs')
}
$zipPath = Join-Path $outputRoot ('Wallet-Studio-' + $Version + '-Windows.zip')
Compress-Archive -LiteralPath $package -DestinationPath $zipPath -Force
$hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest = [ordered]@{version=$Version; platform='windows'; asset=(Split-Path -Leaf $zipPath); sha256=$hash; size=(Get-Item $zipPath).Length; authenticodeStatus=$signature.Status.ToString(); updateMode='manual'; animatedWalletSupport='static-on-tested-device'; includesPrivateRuntimeData=$false}
$manifestPath = Join-Path $outputRoot 'release-manifest.json'
$manifest | ConvertTo-Json | Set-Content -LiteralPath $manifestPath -Encoding utf8
($hash + '  ' + (Split-Path -Leaf $zipPath)) | Set-Content -LiteralPath (Join-Path $outputRoot 'SHA256SUMS.txt') -Encoding ascii
Write-Output $zipPath
Write-Output $manifestPath
