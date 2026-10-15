# DiagnÃ³stico sintÃ©tico v12 con xtool

Esta app separada mide la ruta de shaders y el perfil de visualizaciÃ³n usando objetos y archivos sintÃ©ticos. No abre la biblioteca de pases, no lee ni modifica tarjetas bancarias y no solicita permisos privados de Wallet. `Wallet_animation_proven` permanece en `false`.

## Estado comprobado

- La revisiÃ³n v11 compilÃ³ con xtool y se ejecutÃ³ en un dispositivo fÃ­sico. Su informe pasÃ³ el validador estricto: fuente del shader conservada, controles sintÃ©ticos correctos y vista nativa rechazada. La traza registrÃ³ `liveRenderType`, `displayProfile` y `liveRenderedImageSet`. Esto justifica medir el perfil; no demuestra que su ausencia cause el rechazo.
- La revisiÃ³n v12 compila con cÃ³digo 0 desde Ubuntu 24.04 en WSL2, Swift 6.4.0, xtool 1.22.2 y SDK iPhoneOS 27.0 extraÃ­do de Xcode 27. Su validador pasa 26 pruebas. EjecuciÃ³n fÃ­sica v12 pendiente.
- La revisiÃ³n v12 observa si existe un perfil, compara perfiles construidos con los mismos metadatos pÃºblicos sintÃ©ticos y verifica las firmas del runtime. Si el perfil inicial estÃ¡ ausente y los controles coinciden, vincula un perfil propio, comprueba su lectura posterior y repite el intento de crear la vista. No asigna estilos ni enums bancarios supuestos.
- La revisión v13 se ejecutó: captura acotada del constructor aceptada por el validador; contiene ramas según el tipo derivado del pase, pero 512 bytes no cubren la rama usada. La vista sigue devolviendo nil.
- La revisión v14 verifica la firma y el módulo del constructor `PKLiveRenderedCardFaceView initWithFrame:pass:`. Lee exactamente 4.096 bytes de ese código usando `vm_read_overwrite(mach_task_self(), ...)`, sin obtener puertos de otros procesos ni modificar instrucciones. Reconoce únicamente stubs ARM64 estándar de selectores Objective-C y exporta como máximo 128 nombres con sus desplazamientos dentro de la ventana de código. Comprueba que los nombres son selectores registrados. No exporta direcciones ni rutas. La captura no equivale a una vista creada ni a ejecución de shaders.
- La revisión v14 se ejecutó y su informe pasó el validador. El código de la rama para el tipo derivado 1 obtiene `diffuseMaterialPropertyImage`, `metalnessMaterialPropertyImage`, `normalMaterialPropertyImage` y sus `imageRef`, y llama a `initWithDiffuse:metalness:normal:`. Los nombres resueltos son referencias estáticas dentro de una ventana de código; no equivalen a una traza de llamadas ejecutadas. La ventana puede incluir código contiguo.
- La revisión v15 conserva el intento inicial con solo diffuse. Después de un resultado nil, completa únicamente los otros dos mapas con PNG propios (metal negro y normales planas), verifica las firmas, los getters, tres CGImage no vacíos y la identidad del conjunto. Reintenta la vista y conserva las seis muestras de color. Una vista creada con mapas completos no prueba que se consuma el fragment shader ni que exista animación.
- La revisión v15 se ejecutó dos veces: ambas se detuvieron en `capture_control_failed`, antes del intento con mapas completos. Esas ejecuciones no evalúan la hipótesis de materiales. La revisión v16 mantiene el criterio rojo y añade RGB acotado del control, estado activo de la aplicación y presencia de la ventana. Programa el diagnóstico un segundo después del arranque para observarlo fuera del callback inicial.
- La revisión v16 se ejecutó y su informe pasó el validador: aplicación activa y ventana presente, control rojo capturado como RGB (163,0,0), por lo que la captura siguió sin certificar. No se evaluaron los materiales completos. La revisión v17 fija `preferredRange=Standard`, escala 1 y salida opaca en las imágenes sintéticas y capturas; convierte el píxel mediante sRGB explícito y registra alfa. Mantiene el umbral rojo y exige alfa 255, sin convertir la captura fallida anterior en un resultado positivo.
- Resultado físico v17: captura roja (255,0,0), alfa 255; control inicial con solo diffuse devuelve nil; mapas completos vinculados con tres CGImage válidos y vista candidata creada. Se observa `PKTexturedCardView` y `UIImageView`. Las seis muestras permanecen grises (115,115,115): no se observaron fases temporales. Esto acredita admisión de la vista, no ejecución del shader ni animación en Apple Wallet.
- `source-parity.json` documenta los hashes de los ocho archivos Objective-C de la revisiÃ³n preparada. `Probe.m` conserva los bytes de su `main.m`, renombrado para evitar que SwiftPM lo trate como un objetivo ejecutable.

## Preparar el SDK y compilar

Con Swift 6.4 y xtool instalados en Linux:

```sh
xtool sdk install "$XCODE_XIP" --slim
swift sdk list
xtool dev build --configuration release --ipa
python3 -m unittest discover -s tests -v
```

`XCODE_XIP` debe contener la ruta Linux al archivo obtenido desde Apple. Conviene compilar en el sistema de archivos Linux de WSL. El SDK instalado aparece como `darwin`. El proyecto biblioteca se enlaza con el ejecutable generado por xtool y aporta su funciÃ³n `main`.

No aÃ±adir `--sign`: esta compilaciÃ³n no necesita credenciales de Apple en xtool. El propietario puede firmar e instalar con Sideloadly, introduciendo personalmente sus credenciales cuando corresponda.

## Paquete v17

`xtool/ShaderDiagnostics.ipa` sin firma: 141.951 bytes, SHA-256 `8f5da958080229398f1a526cd82c7f67aedb7290f208ca2990107ceef85e5a61`. ZIP Ã­ntegro, ejecutable Mach-O arm64, mÃ­nimo iOS 17.0, versiÃ³n 1.12 (12), identificador `local.walletstudio.shaderdiagnostics`. La firma posterior cambia el hash y puede cambiar el identificador.

Abrir la app, esperar a Â«Shaders v12 Â· diagnÃ³stico guardadoÂ» y recuperar su propio `Documents/shader-report.json`. Validar el informe antes de extraer conclusiones. Una vista o shader en esta app no prueba animaciÃ³n dentro de Apple Wallet. No publicar informes del dispositivo ni identificadores de firma.

La compilaciÃ³n v11 anterior permanece en el historial del repositorio; su IPA sin firma tenÃ­a SHA-256 `70cae006bffee0b55a321ae5a1c34b4ebf1e9e6da545729e2f24ad816cdbaa23`.

## Fuentes

- https://github.com/xtool-org/xtool/tree/1.22.2
- https://xtool.sh/documentation/xtool/installation-linux/
- `Sources/PackLib/Packer.swift`: genera un ejecutable C que depende de un producto biblioteca.
- `Sources/XToolSupport/DevCommand.swift`: permite `build --ipa` sin credenciales de firma.
- DeclaraciÃ³n de referencia de `PKPassDisplayProfile`; cada firma usada se vuelve a comprobar en el runtime: https://github.com/MTACS/iOS-17-Runtime-Headers/blob/d1d960dfaa4107765dd7fcf891e4967c0930d5fd/PrivateFrameworks/PassKitCore.framework/PKPassDisplayProfile.h
