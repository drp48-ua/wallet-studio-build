$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'distribution-common.ps1')
$projectRoot = Split-Path -Parent $PSScriptRoot
$script:passed = 0
function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:passed++
}
function Assert-Throws([scriptblock]$Action, [string]$Pattern) {
    $message = $null
    try { & $Action | Out-Null } catch { $message = $_.Exception.Message }
    Assert-True ($null -ne $message -and $message -match $Pattern) "No se rechazó correctamente: $Pattern (resultado: $message)"
}
Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.ps1' | ForEach-Object {
    $tokens=$null; $errors=$null
    [System.Management.Automation.Language.Parser]::ParseFile($_.FullName,[ref]$tokens,[ref]$errors) | Out-Null
    if ($errors.Count) { throw "Sintaxis incorrecta: $($_.Name): $errors" }
}
$releaseVersion = Get-WalletVersion $projectRoot
Assert-True ($releaseVersion -match '^\d+\.\d+\.\d+$') 'Las versiones no coinciden'
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('wallet-distribution-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
try {
    foreach ($dir in @('distribution','distribution/design-licenses','windows/designs','third-party-licenses','windows-source')) {
        New-Item -ItemType Directory -Path (Join-Path $fixture $dir) -Force | Out-Null
    }
    '{"files":[{"source":"windows/app.exe","destination":"Wallet Studio.exe"}]}' | Set-Content (Join-Path $fixture 'distribution/files.json')
    '{"designs":[]}' | Set-Content (Join-Path $fixture 'distribution/designs.json')
    '{"crates":[]}' | Set-Content (Join-Path $fixture 'third-party-licenses/INDEX.json')
    foreach ($file in @('windows/app.exe','windows/designs/README.md','third-party-licenses/LICENSE-Windows-handshake.txt','windows/designs/guardado-synthetic.png','windows/designs/private.json','windows/designs/private.png','windows/session.log')) {
        'synthetic test data' | Set-Content -LiteralPath (Join-Path $fixture $file)
    }
    $plan = @(Get-DistributionPlan $fixture)
    Assert-True ($plan.Count -eq 4 -and -not ($plan.Source -match 'private|guardado|session')) 'Se filtraron archivos fuera de la lista'
    Assert-Throws { Get-SafeDistributionFile $fixture '../LICENSE' } 'Ruta no permitida'
    Assert-Throws { Get-SafeDistributionFile $fixture 'C:/outside.txt' } 'Ruta no permitida'
    $junction = Join-Path $fixture 'linked'
    New-Item -ItemType Junction -Path $junction -Target (Join-Path $fixture 'windows') | Out-Null
    Assert-Throws { Get-SafeDistributionFile $fixture 'linked/app.exe' } 'No se permiten enlaces'
    [IO.Directory]::Delete($junction)
    'synthetic approved design' | Set-Content (Join-Path $fixture 'windows/designs/approved.png')
    'synthetic permission notice' | Set-Content (Join-Path $fixture 'distribution/design-licenses/approved.txt')
    $hash = (Get-FileHash (Join-Path $fixture 'windows/designs/approved.png')).Hash
    @{ designs=@(@{file='approved.png';sha256=$hash;rights='Synthetic test fixture';licenseFile='distribution/design-licenses/approved.txt'}) } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $fixture 'distribution/designs.json')
    $plan = @(Get-DistributionPlan $fixture)
    Assert-True ($plan.Relative -contains 'designs/approved.png' -and $plan.Relative -contains 'designs/approved.txt') 'Falta diseño aprobado o aviso'
    'modified' | Set-Content (Join-Path $fixture 'windows/designs/approved.png')
    Assert-Throws { Get-DistributionPlan $fixture } 'diseño autorizado cambió'
    @'
[[package]]
name = "missing"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
'@ | Set-Content (Join-Path $fixture 'windows-source/Cargo.lock')
    Assert-True (@(Get-LicenseGaps $fixture) -contains 'missing 1.0.0') 'No se detectó dependencia sin aviso'
    Copy-Item (Join-Path $projectRoot 'windows-source/Cargo.toml') (Join-Path $fixture 'windows-source/Cargo.toml')
    Copy-Item (Join-Path $projectRoot 'windows-source/Cargo.lock') (Join-Path $fixture 'windows-source/Cargo.lock')
    (Get-Content (Join-Path $projectRoot 'wallet-studio.rc') -Raw).Replace($releaseVersion,'9.9.9') | Set-Content (Join-Path $fixture 'wallet-studio.rc')
    Assert-Throws { Get-WalletVersion $fixture } 'Versión incoherente'
    $fakeZip = Join-Path $fixture 'fake.zip'
    'not a valid archive' | Set-Content $fakeZip
    Assert-Throws { & (Join-Path $PSScriptRoot 'verify-download.ps1') -ZipPath $fakeZip -ExpectedVersion 0.5.2 -CurrentVersion 0.5.3 -ExpectedSha256 ('0'*64) -ExpectedSignerThumbprint ('0'*40) } 'debe ser posterior'
    Assert-Throws { & (Join-Path $PSScriptRoot 'verify-download.ps1') -ZipPath $fakeZip -ExpectedVersion 0.5.4 -CurrentVersion 0.5.3 -ExpectedSha256 ('0'*64) -ExpectedSignerThumbprint ('0'*40) } 'SHA-256 incorrecto'
    $workflow = Get-Content (Join-Path $projectRoot '.github/workflows/windows.yml') -Raw
    Assert-True ($workflow -notmatch 'upload-artifact|gh release|action-gh-release') 'El workflow Windows publica artefactos'
    Write-Output "$script:passed comprobaciones de distribución correctas (datos sintéticos; sin compilar ni probar USB)."
} finally {
    $resolvedFixture = [IO.Path]::GetFullPath($fixture)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolvedFixture.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolvedFixture) -notlike 'wallet-distribution-test-*') { throw 'Ruta de limpieza fuera del directorio temporal de pruebas' }
    if (Test-Path -LiteralPath (Join-Path $fixture 'linked')) { [IO.Directory]::Delete((Join-Path $fixture 'linked')) }
    Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
}
