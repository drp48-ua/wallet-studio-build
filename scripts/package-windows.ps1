param([string]$Version = '0.6.10', [switch]$RequireSigned, [string]$BinaryPath = '')
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Version no valida' }
$projectRoot = Split-Path -Parent $PSScriptRoot
$binary = if ($BinaryPath) { (Resolve-Path -LiteralPath $BinaryPath).Path } else { Join-Path $projectRoot 'windows/Wallet Studio.exe' }
if ((Get-Item -LiteralPath $binary).VersionInfo.FileVersion -ne $Version) { throw 'La version del ejecutable no coincide' }
$signature = Get-AuthenticodeSignature -LiteralPath $binary
if ($RequireSigned -and $signature.Status -ne 'Valid') { throw 'La distribucion requiere una firma Authenticode valida' }
$outputRoot = Join-Path $projectRoot 'dist'
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
$staging = Join-Path $outputRoot ('stage-' + [Guid]::NewGuid().ToString('N'))
$package = Join-Path $staging ('Wallet-Studio-' + $Version)
New-Item -ItemType Directory -Path (Join-Path $package 'designs') -Force | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $package 'Wallet Studio.exe')
foreach ($relative in @('windows/ABRIR-WALLET-STUDIO.cmd', 'windows/LICENSE-AirCard-Windows.txt')) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $relative) -Destination $package
}
foreach ($relative in @('LICENSE','LEEME.md','ACTUALIZACIONES.md','CLIENTES-Y-NUBE.md','SEGURIDAD.md','ANIMACION-Y-WALLET.md','PRUEBA-WINDOWS.md',('NOVEDADES-' + $Version + '.md'))) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $relative) -Destination $package
}
Copy-Item -LiteralPath (Join-Path $projectRoot 'third-party-licenses') -Destination $package -Recurse
# Paid catalog images are provisioned privately, never copied into releases.
Copy-Item -LiteralPath (Join-Path $projectRoot 'windows/designs/README.md') -Destination (Join-Path $package 'designs')
$zipPath = Join-Path $outputRoot ('Wallet-Studio-' + $Version + '-Windows.zip')
Compress-Archive -LiteralPath $package -DestinationPath $zipPath -Force
$hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest = [ordered]@{version=$Version; platform='windows'; asset=(Split-Path -Leaf $zipPath); sha256=$hash; size=(Get-Item $zipPath).Length; authenticodeStatus=$signature.Status.ToString(); updateMode='signed-on-restart-after-channel-setup'; animatedWalletSupport='static-on-tested-device'; includesPrivateRuntimeData=$false}
$manifestPath = Join-Path $outputRoot 'release-manifest.json'
[System.IO.File]::WriteAllText($manifestPath,($manifest | ConvertTo-Json),(New-Object System.Text.UTF8Encoding($false)))
($hash + '  ' + (Split-Path -Leaf $zipPath)) | Set-Content -LiteralPath (Join-Path $outputRoot 'SHA256SUMS.txt') -Encoding ascii
Write-Output $zipPath
Write-Output $manifestPath
