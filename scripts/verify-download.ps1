param(
    [Parameter(Mandatory)][string]$ZipPath,
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$ExpectedVersion,
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$CurrentVersion,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedSha256,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{40}$')][string]$ExpectedSignerThumbprint
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# No ejecuta ni sustituye el programa. Datos esperados obtenidos por el canal de compra.
if ([version]$ExpectedVersion -le [version]$CurrentVersion) { throw 'La actualización debe ser posterior; no se permite retroceder ni reinstalar con este comprobador' }
if ((Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash -ne $ExpectedSha256) { throw 'SHA-256 incorrecto; no abrir ni instalar' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $ZipPath).Path)
$tempExe = Join-Path ([IO.Path]::GetTempPath()) ('wallet-verify-' + [guid]::NewGuid().ToString('N') + '.exe')
$createdTempExe = $false
try {
    $prefix = "Wallet-Studio-$ExpectedVersion-Windows/"
    $names = @{}
    foreach ($entry in $archive.Entries) {
        $name = $entry.FullName.Replace('\','/')
        if (-not $name.StartsWith($prefix, [StringComparison]::Ordinal) -or $name.Contains(':') -or
            @($name.Split('/') | Where-Object { $_ -in @('.', '..') }).Count -or $names.ContainsKey($name)) { throw 'ZIP con rutas no válidas o duplicadas' }
        $names[$name] = $entry
    }
    $versionEntry = $names[$prefix + 'VERSION.json']
    if (-not $versionEntry -or $versionEntry.Length -gt 1048576) { throw 'Falta información de versión válida' }
    $reader = [IO.StreamReader]::new($versionEntry.Open())
    try { $info = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
    if ($info.version -ne $ExpectedVersion -or $info.platform -ne 'windows-x64' -or $info.draft) { throw 'Versión, plataforma o estado de entrega incorrectos' }
    $exe = $names[$prefix + 'Wallet Studio.exe']
    if (-not $exe -or $exe.Length -gt 536870912) { throw 'Ejecutable ausente o demasiado grande' }
    $inputStream = $exe.Open()
    try {
        $outputStream = [IO.File]::Open($tempExe, [IO.FileMode]::CreateNew)
        $createdTempExe = $true
        try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose() }
    } finally { $inputStream.Dispose() }
    if ((Get-Item -LiteralPath $tempExe).VersionInfo.FileVersion -notmatch ('^' + [regex]::Escape($ExpectedVersion) + '(?:\.0)?$')) { throw 'El ejecutable tiene otra versión' }
    $signature = Get-AuthenticodeSignature -LiteralPath $tempExe
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $ExpectedSignerThumbprint) { throw 'Firma o editor incorrectos; no instalar' }
    Write-Output "Verificado Wallet Studio $ExpectedVersion. No se ha sustituido ni ejecutado ningún archivo. Sigue ACTUALIZACIONES.md."
} finally {
    $archive.Dispose()
    if ($createdTempExe -and (Test-Path -LiteralPath $tempExe)) { Remove-Item -LiteralPath $tempExe }
}
