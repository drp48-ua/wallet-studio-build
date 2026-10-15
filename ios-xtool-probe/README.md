# Diagn贸stico sint茅tico v12 con xtool

Esta app separada mide la ruta de shaders y el perfil de visualizaci贸n usando objetos y archivos sint茅ticos. No abre la biblioteca de pases, no lee ni modifica tarjetas bancarias y no solicita permisos privados de Wallet. `Wallet_animation_proven` permanece en `false`.

## Estado comprobado

- La revisi贸n v11 compil贸 con xtool y se ejecut贸 en un dispositivo f铆sico. Su informe pas贸 el validador estricto: fuente del shader conservada, controles sint茅ticos correctos y vista nativa rechazada. La traza registr贸 `liveRenderType`, `displayProfile` y `liveRenderedImageSet`. Esto justifica medir el perfil; no demuestra que su ausencia cause el rechazo.
- La revisi贸n v12 compila con c贸digo 0 desde Ubuntu 24.04 en WSL2, Swift 6.4.0, xtool 1.22.2 y SDK iPhoneOS 27.0 extra铆do de Xcode 27. Su validador pasa 26 pruebas. Ejecuci贸n f铆sica v12 pendiente.
- La revisi贸n v12 observa si existe un perfil, compara perfiles construidos con los mismos metadatos p煤blicos sint茅ticos y verifica las firmas del runtime. Si el perfil inicial est谩 ausente y los controles coinciden, vincula un perfil propio, comprueba su lectura posterior y repite el intento de crear la vista. No asigna estilos ni enums bancarios supuestos.
- La revisi髇 v13 verifica la firma y el m骴ulo del constructor `PKLiveRenderedCardFaceView initWithFrame:pass:`. Lee exactamente 512 bytes de ese c骴igo usando `vm_read_overwrite(mach_task_self(), ...)`, sin obtener puertos de otros procesos ni modificar instrucciones. No exporta direcciones ni rutas. La captura no equivale a una vista creada ni a ejecuci髇 de shaders.
- `source-parity.json` documenta los hashes de los siete archivos Objective-C de la revisi贸n preparada. `Probe.m` conserva los bytes de su `main.m`, renombrado para evitar que SwiftPM lo trate como un objetivo ejecutable.

## Preparar el SDK y compilar

Con Swift 6.4 y xtool instalados en Linux:

```sh
xtool sdk install "$XCODE_XIP" --slim
swift sdk list
xtool dev build --configuration release --ipa
python3 -m unittest discover -s tests -v
```

`XCODE_XIP` debe contener la ruta Linux al archivo obtenido desde Apple. Conviene compilar en el sistema de archivos Linux de WSL. El SDK instalado aparece como `darwin`. El proyecto biblioteca se enlaza con el ejecutable generado por xtool y aporta su funci贸n `main`.

No a帽adir `--sign`: esta compilaci贸n no necesita credenciales de Apple en xtool. El propietario puede firmar e instalar con Sideloadly, introduciendo personalmente sus credenciales cuando corresponda.

## Paquete v13

`xtool/ShaderDiagnostics.ipa` sin firma: 140.207 bytes, SHA-256 `678fdb55b4db190ac4f37205dbc869b31c44dc340392e441fdc6b7a21bea4755`. ZIP 铆ntegro, ejecutable Mach-O arm64, m铆nimo iOS 17.0, versi贸n 1.12 (12), identificador `local.walletstudio.shaderdiagnostics`. La firma posterior cambia el hash y puede cambiar el identificador.

Abrir la app, esperar a 芦Shaders v12 路 diagn贸stico guardado禄 y recuperar su propio `Documents/shader-report.json`. Validar el informe antes de extraer conclusiones. Una vista o shader en esta app no prueba animaci贸n dentro de Apple Wallet. No publicar informes del dispositivo ni identificadores de firma.

La compilaci贸n v11 anterior permanece en el historial del repositorio; su IPA sin firma ten铆a SHA-256 `70cae006bffee0b55a321ae5a1c34b4ebf1e9e6da545729e2f24ad816cdbaa23`.

## Fuentes

- https://github.com/xtool-org/xtool/tree/1.22.2
- https://xtool.sh/documentation/xtool/installation-linux/
- `Sources/PackLib/Packer.swift`: genera un ejecutable C que depende de un producto biblioteca.
- `Sources/XToolSupport/DevCommand.swift`: permite `build --ipa` sin credenciales de firma.
- Declaraci贸n de referencia de `PKPassDisplayProfile`; cada firma usada se vuelve a comprobar en el runtime: https://github.com/MTACS/iOS-17-Runtime-Headers/blob/d1d960dfaa4107765dd7fcf891e4967c0930d5fd/PrivateFrameworks/PassKitCore.framework/PKPassDisplayProfile.h
