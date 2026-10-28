$ErrorActionPreference = 'Stop'
$helper = Join-Path $PSScriptRoot 'prepare-release-alias.ps1'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('wallet-release-alias-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
$rsa = [Security.Cryptography.RSA]::Create(3072)
$passed = 0
try {
    $parameters = $rsa.ExportParameters($false)
    [byte[]]$header = [BitConverter]::GetBytes([uint32]0x31415352) + [BitConverter]::GetBytes([uint32]3072) + [BitConverter]::GetBytes([uint32]3) + [BitConverter]::GetBytes([uint32]384) + [BitConverter]::GetBytes([uint32]0) + [BitConverter]::GetBytes([uint32]0)
    $publicPath = Join-Path $testRoot 'synthetic-public.blob'
    [IO.File]::WriteAllBytes($publicPath,([byte[]]($header + $parameters.Exponent + $parameters.Modulus)))
    foreach ($scenario in @('valid','missing-signature','changed-manifest','corrupt-zip','wrong-key','private-runtime','wrong-branch','path-escape','wrong-size','wrong-version','occupied-alias')) {
        $directory = Join-Path $testRoot $scenario
        New-Item -ItemType Directory -Path $directory | Out-Null
        $zip = Join-Path $directory 'Wallet-Studio-9.9.9-Windows.zip'
        [IO.File]::WriteAllBytes($zip,([Text.Encoding]::UTF8.GetBytes('PK synthetic public build bytes')))
        $manifest = [ordered]@{ version='9.9.9'; platform='windows'; source_branch='main'; asset='Wallet-Studio-9.9.9-Windows.zip'; sha256=(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant(); size=(Get-Item $zip).Length; includesPrivateRuntimeData=$false }
        if ($scenario -eq 'private-runtime') { $manifest.includesPrivateRuntimeData=$true }
        if ($scenario -eq 'wrong-branch') { $manifest.source_branch='other' }
        if ($scenario -eq 'path-escape') { $manifest.asset='../Wallet-Studio-9.9.9-Windows.zip' }
        if ($scenario -eq 'wrong-size') { $manifest.size++ }
        if ($scenario -eq 'wrong-version') { $manifest.version='9.9.8' }
        $manifestPath = Join-Path $directory 'release-manifest.json'
        $bytes = [Text.Encoding]::UTF8.GetBytes(($manifest | ConvertTo-Json))
        [IO.File]::WriteAllBytes($manifestPath,$bytes)
        $signature = $rsa.SignData($bytes,[Security.Cryptography.HashAlgorithmName]::SHA256,[Security.Cryptography.RSASignaturePadding]::Pss)
        [IO.File]::WriteAllBytes((Join-Path $directory 'release-manifest.sig'),$signature)
        if ($scenario -eq 'missing-signature') { Remove-Item -LiteralPath (Join-Path $directory 'release-manifest.sig') }
        if ($scenario -eq 'changed-manifest') { [IO.File]::AppendAllText($manifestPath,' ') }
        if ($scenario -eq 'corrupt-zip') { [IO.File]::AppendAllText($zip,'changed') }
        if ($scenario -eq 'wrong-key') { $signature[0] = $signature[0] -bxor 255; [IO.File]::WriteAllBytes((Join-Path $directory 'release-manifest.sig'),$signature) }
        $alias = Join-Path $directory 'Wallet-Studio-Windows.zip'
        if ($scenario -eq 'occupied-alias') { [IO.File]::WriteAllText($alias,'retained unrelated bytes') }
        $failed = $false
        try { & $helper -ManifestPath $manifestPath -PublicKeyPath $publicPath | Out-Null } catch { $failed=$true }
        if ($scenario -eq 'valid') {
            if ($failed -or (Get-FileHash $alias).Hash -ne (Get-FileHash $zip).Hash) { throw 'Valid signed ZIP was not copied exactly' }
            & $helper -ManifestPath $manifestPath -PublicKeyPath $publicPath | Out-Null
        } else {
            if (-not $failed) { throw "Invalid fixture was accepted: $scenario" }
            if ($scenario -eq 'occupied-alias') { if ([IO.File]::ReadAllText($alias) -cne 'retained unrelated bytes') { throw 'Existing alias was overwritten' } }
            elseif (Test-Path -LiteralPath $alias) { throw "Invalid fixture produced a stable alias: $scenario" }
        }
        $passed++
    }
    Write-Output "Stable release alias: $passed synthetic cases passed; no network or publisher private key used."
} finally {
    $rsa.Dispose()
    $resolvedRoot = [IO.Path]::GetFullPath($testRoot)
    $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolvedRoot.StartsWith($temporaryRoot,[StringComparison]::OrdinalIgnoreCase) -or -not ([IO.Path]::GetFileName($resolvedRoot)).StartsWith('wallet-release-alias-')) { throw 'Refuse unsafe fixture cleanup' }
    Remove-Item -LiteralPath $resolvedRoot -Recurse -Force
}
