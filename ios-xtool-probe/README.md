# Diagnóstico sintético v11 con xtool

Adaptación de compilación del diagnóstico v11 existente. `Probe.m` conserva los bytes de `shader-probe/main.m`; los cuatro encabezados internos también se conservan sin cambios. El nombre evita que SwiftPM lo trate como un objetivo ejecutable en lugar del producto biblioteca requerido por xtool. El enlazador del ejecutable generado por xtool debe resolver el símbolo `main` de esta biblioteca.

**Estado:** configuración preparada; todavía no compilada con SwiftPM/xtool ni probada en un iPhone. Las pruebas Python del diagnóstico no sustituyen la compilación nativa.

Esta revisión reproduce primero el control v11 en el dispositivo. Es una app separada que solo usa objetos sintéticos y archivos propios. No abre la biblioteca de pases, no lee ni modifica tarjetas bancarias y no solicita permisos privados de Wallet. `Wallet_animation_proven` permanece en `false`.

## Tras completar WSL, Swift 6.4 y el SDK

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

## Fuentes de la adaptación

- https://github.com/xtool-org/xtool/tree/1.22.2
- https://xtool.sh/documentation/xtool/installation-linux/
- `Sources/PackLib/Packer.swift` del repositorio xtool: genera un ejecutable C que depende de un producto biblioteca.
- `Sources/XToolSupport/DevCommand.swift`: permite `build --ipa` sin `--sign`.
