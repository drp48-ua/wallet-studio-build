param(
    [Parameter(Mandatory)][string]$ManifestPath,
    [string]$PublicKeyPath = (Join-Path (Split-Path -Parent $PSScriptRoot) 'windows-source/assets/update-public-key.blob')
)
$ErrorActionPreference = 'Stop'
$manifestFile = Get-Item -LiteralPath $ManifestPath
if ($manifestFile.PSIsContainer -or ($manifestFile.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unsafe release manifest' }
$directory = $manifestFile.DirectoryName
$bytes = [IO.File]::ReadAllBytes($manifestFile.FullName)
$signature = [IO.File]::ReadAllBytes((Join-Path $directory 'release-manifest.sig'))
$public = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $PublicKeyPath))
# BCRYPT_RSAPUBLIC_BLOB: fixed publisher key used by the desktop updater.
if ($public.Length -ne 411 -or [BitConverter]::ToUInt32($public,0) -ne 0x31415352 -or [BitConverter]::ToUInt32($public,4) -ne 3072 -or [BitConverter]::ToUInt32($public,8) -ne 3 -or [BitConverter]::ToUInt32($public,12) -ne 384) { throw 'Unsupported publisher public key' }
$rsa = [Security.Cryptography.RSA]::Create()
try {
    $parameters = [Security.Cryptography.RSAParameters]::new()
    $parameters.Exponent = [byte[]]$public[24..26]
    $parameters.Modulus = [byte[]]$public[27..410]
    $rsa.ImportParameters($parameters)
    if (-not $rsa.VerifyData($bytes,$signature,[Security.Cryptography.HashAlgorithmName]::SHA256,[Security.Cryptography.RSASignaturePadding]::Pss)) { throw 'Invalid release manifest signature' }
} finally { $rsa.Dispose() }
$manifest = [Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json
if ($manifest.platform -ne 'windows' -or $manifest.includesPrivateRuntimeData -isnot [bool] -or $manifest.includesPrivateRuntimeData -ne $false -or $manifest.source_branch -ne 'main' -or $manifest.asset -cnotmatch '^Wallet-Studio-\d+\.\d+\.\d+-Windows\.zip$' -or $manifest.version -cnotmatch '^\d+\.\d+\.\d+$' -or $manifest.asset -cne "Wallet-Studio-$($manifest.version)-Windows.zip" -or $manifest.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $manifest.size -le 0) { throw 'Invalid public Windows release metadata' }
$source = Get-Item -LiteralPath (Join-Path $directory $manifest.asset)
if ($source.PSIsContainer -or ($source.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $source.Length -ne $manifest.size -or (Get-FileHash -LiteralPath $source.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne $manifest.sha256) { throw 'Signed release ZIP integrity mismatch' }
$alias = Join-Path $directory 'Wallet-Studio-Windows.zip'
if (-not (Test-Path -LiteralPath $alias)) { [IO.File]::Copy($source.FullName,$alias,$false) }
$published = Get-Item -LiteralPath $alias
if ($published.PSIsContainer -or ($published.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $published.Length -ne $source.Length -or (Get-FileHash -LiteralPath $alias -Algorithm SHA256).Hash.ToLowerInvariant() -cne $manifest.sha256) { throw 'Stable alias differs from signed ZIP' }
Write-Output $alias
