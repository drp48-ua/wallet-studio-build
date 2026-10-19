param([ValidateSet('Runtime','Build','All')][string]$Mode = 'All', [switch]$RequireReady)
$ErrorActionPreference = 'Stop'
$checks = @()
if ($Mode -in @('Runtime','All')) {
    $dirs = @('C:\Program Files\Common Files\Apple\Mobile Device Support', 'C:\Program Files (x86)\Common Files\Apple\Mobile Device Support')
    $dlls = @('CoreFoundation.dll','MobileDevice.dll','AirTrafficHost.dll')
    $complete = $false
    foreach ($dir in $dirs) {
        $present = @($dlls | Where-Object { Test-Path -LiteralPath (Join-Path $dir $_) -PathType Leaf })
        if ($present.Count -eq 3) { $complete = $true }
        foreach ($dll in $dlls) { Write-Host "$dir\${dll}: $($dll -in $present)" }
    }
    $checks += [pscustomobject]@{ Component='Archivos Apple (no prueba de carga ni USB)'; Ready=$complete }
    if (-not $complete) { Write-Warning 'Instala o repara el software oficial de Apple: https://support.apple.com/es-es/118290. Consulta INSTALACION.md; no descargues DLL sueltas.' }
}
if ($Mode -in @('Build','All')) {
    foreach ($tool in @('rustup','cargo','rustc','cl.exe','link.exe','rc.exe')) {
        $found = Get-Command $tool -ErrorAction SilentlyContinue
        $checks += [pscustomobject]@{ Component=$tool; Ready=($null -ne $found) }
    }
    if (Get-Command rustup -ErrorAction SilentlyContinue) {
        $compiler = & rustup run stable rustc -vV 2>&1
        $stable = $LASTEXITCODE -eq 0 -and ($compiler -match 'host: x86_64-pc-windows-msvc')
        $checks += [pscustomobject]@{ Component='Rust estable x64 MSVC'; Ready=[bool]$stable }
    }
    Write-Host 'Compilación: Rust estable (https://rustup.rs) y consola x64 de Visual Studio Build Tools con C++ y Windows SDK. El cliente final no necesita estas herramientas.'
}
$checks | Format-Table -AutoSize | Out-Host
if ($RequireReady -and @($checks | Where-Object { -not $_.Ready }).Count) { throw 'Faltan dependencias; consulta el diagnóstico anterior.' }
