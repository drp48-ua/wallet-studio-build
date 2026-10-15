# Diagn√≥stico sint√©tico v12 con xtool

Esta app separada mide la ruta de shaders y el perfil de visualizaci√≥n usando objetos y archivos sint√©ticos. No abre la biblioteca de pases, no lee ni modifica tarjetas bancarias y no solicita permisos privados de Wallet. `Wallet_animation_proven` permanece en `false`.

## Estado comprobado

- La revisi√≥n v11 compil√≥ con xtool y se ejecut√≥ en un dispositivo f√≠sico. Su informe pas√≥ el validador estricto: fuente del shader conservada, controles sint√©ticos correctos y vista nativa rechazada. La traza registr√≥ `liveRenderType`, `displayProfile` y `liveRenderedImageSet`. Esto justifica medir el perfil; no demuestra que su ausencia cause el rechazo.
- La revisi√≥n v12 compila con c√≥digo 0 desde Ubuntu 24.04 en WSL2, Swift 6.4.0, xtool 1.22.2 y SDK iPhoneOS 27.0 extra√≠do de Xcode 27. Su validador pasa 26 pruebas. Ejecuci√≥n f√≠sica v12 pendiente.
- La revisi√≥n v12 observa si existe un perfil, compara perfiles construidos con los mismos metadatos p√∫blicos sint√©ticos y verifica las firmas del runtime. Si el perfil inicial est√° ausente y los controles coinciden, vincula un perfil propio, comprueba su lectura posterior y repite el intento de crear la vista. No asigna estilos ni enums bancarios supuestos.
- La revisiÛn v13 se ejecutÛ: captura acotada del constructor aceptada por el validador; contiene ramas seg˙n el tipo derivado del pase, pero 512 bytes no cubren la rama usada. La vista sigue devolviendo nil.
- La revisiÛn v14 verifica la firma y el mÛdulo del constructor `PKLiveRenderedCardFaceView initWithFrame:pass:`. Lee exactamente 4.096 bytes de ese cÛdigo usando `vm_read_overwrite(mach_task_self(), ...)`, sin obtener puertos de otros procesos ni modificar instrucciones. Reconoce ˙nicamente stubs ARM64 est·ndar de selectores Objective-C y exporta como m·ximo 128 nombres con sus desplazamientos dentro de la ventana de cÛdigo. Comprueba que los nombres son selectores registrados. No exporta direcciones ni rutas. La captura no equivale a una vista creada ni a ejecuciÛn de shaders.
- La revisiÛn v14 se ejecutÛ y su informe pasÛ el validador. El cÛdigo de la rama para el tipo derivado 1 obtiene `diffuseMaterialPropertyImage`, `metalnessMaterialPropertyImage`, `normalMaterialPropertyImage` y sus `imageRef`, y llama a `initWithDiffuse:metalness:normal:`. Los nombres resueltos son referencias est·ticas dentro de una ventana de cÛdigo; no equivalen a una traza de llamadas ejecutadas. La ventana puede incluir cÛdigo contiguo.
- La revisiÛn v15 conserva el intento inicial con solo diffuse. DespuÈs de un resultado nil, completa ˙nicamente los otros dos mapas con PNG propios (metal negro y normales planas), verifica las firmas, los getters, tres CGImage no vacÌos y la identidad del conjunto. Reintenta la vista y conserva las seis muestras de color. Una vista creada con mapas completos no prueba que se consuma el fragment shader ni que exista animaciÛn.
- `source-parity.json` documenta los hashes de los ocho archivos Objective-C de la revisi√≥n preparada. `Probe.m` conserva los bytes de su `main.m`, renombrado para evitar que SwiftPM lo trate como un objetivo ejecutable.

## Preparar el SDK y compilar

Con Swift 6.4 y xtool instalados en Linux:

```sh
xtool sdk install "$XCODE_XIP" --slim
swift sdk list
xtool dev build --configuration release --ipa
python3 -m unittest discover -s tests -v
```

`XCODE_XIP` debe contener la ruta Linux al archivo obtenido desde Apple. Conviene compilar en el sistema de archivos Linux de WSL. El SDK instalado aparece como `darwin`. El proyecto biblioteca se enlaza con el ejecutable generado por xtool y aporta su funci√≥n `main`.

No a√±adir `--sign`: esta compilaci√≥n no necesita credenciales de Apple en xtool. El propietario puede firmar e instalar con Sideloadly, introduciendo personalmente sus credenciales cuando corresponda.

## Paquete v15

`xtool/ShaderDiagnostics.ipa` sin firma: 141.183 bytes, SHA-256 `0fe0d718e2ca7598e6959dd0abba1e6dbff1b02904aa23b233700b66fa8cdf3b`. ZIP √≠ntegro, ejecutable Mach-O arm64, m√≠nimo iOS 17.0, versi√≥n 1.12 (12), identificador `local.walletstudio.shaderdiagnostics`. La firma posterior cambia el hash y puede cambiar el identificador.

Abrir la app, esperar a ¬´Shaders v12 ¬∑ diagn√≥stico guardado¬ª y recuperar su propio `Documents/shader-report.json`. Validar el informe antes de extraer conclusiones. Una vista o shader en esta app no prueba animaci√≥n dentro de Apple Wallet. No publicar informes del dispositivo ni identificadores de firma.

La compilaci√≥n v11 anterior permanece en el historial del repositorio; su IPA sin firma ten√≠a SHA-256 `70cae006bffee0b55a321ae5a1c34b4ebf1e9e6da545729e2f24ad816cdbaa23`.

## Fuentes

- https://github.com/xtool-org/xtool/tree/1.22.2
- https://xtool.sh/documentation/xtool/installation-linux/
- `Sources/PackLib/Packer.swift`: genera un ejecutable C que depende de un producto biblioteca.
- `Sources/XToolSupport/DevCommand.swift`: permite `build --ipa` sin credenciales de firma.
- Declaraci√≥n de referencia de `PKPassDisplayProfile`; cada firma usada se vuelve a comprobar en el runtime: https://github.com/MTACS/iOS-17-Runtime-Headers/blob/d1d960dfaa4107765dd7fcf891e4967c0930d5fd/PrivateFrameworks/PassKitCore.framework/PKPassDisplayProfile.h
