# Diagnóstico sintético v11 con xtool

Adaptación de compilación del diagnóstico v11 existente. `Probe.m` conserva los bytes de `shader-probe/main.m`; los cuatro encabezados internos también se conservan sin cambios. El nombre evita que SwiftPM lo trate como un objetivo ejecutable en lugar del producto biblioteca requerido por xtool. El enlazador del ejecutable generado por xtool debe resolver el símbolo `main` de esta biblioteca.

**Estado:** compilación cruzada iOS realizada con éxito el 15 de octubre de 2026 desde Ubuntu 24.04 en WSL2, Swift 6.4.0, xtool 1.22.2 y SDK iPhoneOS 27.0 extraído de Xcode 27. El paquete contiene un ejecutable Mach-O arm64 para iOS 17.0 o posterior. Firma, instalación y ejecución física pendientes. Las pruebas Python del diagnóstico no sustituyen esa ejecución.

Esta revisión reproduce primero el control v11 en el dispositivo. Es una app separada que solo usa objetos sintéticos y archivos propios. No abre la biblioteca de pases, no lee ni modifica tarjetas bancarias y no solicita permisos privados de Wallet. `Wallet_animation_proven` permanece en `false`.

## Preparar el SDK y compilar

Desde Ubuntu, instalar el SDK del archivo descargado desde Apple:

```sh
xtool sdk install "$XCODE_XIP" --slim
swift sdk list
```

Desde este directorio del proyecto:

```sh
xtool dev build --configuration release --ipa
```

`XCODE_XIP` debe contener la ruta Linux al archivo descargado; en WSL, las unidades de Windows aparecen bajo `/mnt/c`, `/mnt/d`, etc.

No añadir `--sign`: esta compilación no necesita credenciales de Apple en xtool. La firma e instalación pueden realizarse mediante el flujo de Sideloadly ya utilizado por el propietario. La app escribe `shader-report.json` en sus propios Documentos. El informe debe validarse antes de extraer conclusiones; una vista o shader en esta app no prueba animación dentro de Apple Wallet.

El siguiente diagnóstico de perfil de visualización requiere confirmar primero el resultado v11 en el dispositivo. No se eligen enums privados ni se fabrican campos del emisor para forzar un resultado positivo.

## Resultado de compilación local

`swift sdk list` devuelve `darwin`. `xtool dev build --configuration release --ipa` terminó con código 0 y generó `xtool/ShaderDiagnostics.ipa`. No hubo que cambiar los cinco archivos Objective-C de referencia para compilar ni enlazar el punto de entrada `main`.

El IPA sin firma tiene 139.511 bytes y SHA-256 `70cae006bffee0b55a321ae5a1c34b4ebf1e9e6da545729e2f24ad816cdbaa23`. Incluye únicamente `Info.plist` y el ejecutable dentro de `Payload/ShaderDiagnostics.app`. Identificador de paquete `local.walletstudio.shaderdiagnostics`, versión 1.11 (11), plataforma iPhoneOS, capacidad arm64, documentos compartidos habilitados. La firma posterior cambia el contenido y el hash del paquete. Compilar este diagnóstico no demuestra animación en Apple Wallet.

## Fuentes de la adaptación

- https://github.com/xtool-org/xtool/tree/1.22.2
- https://xtool.sh/documentation/xtool/installation-linux/
- `Sources/PackLib/Packer.swift` del repositorio xtool: genera un ejecutable C que depende de un producto biblioteca.
- `Sources/XToolSupport/DevCommand.swift`: permite `build --ipa` sin `--sign`.
