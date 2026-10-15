# Diagnóstico sintético v12 con xtool

Esta app separada mide la ruta de shaders y el perfil de visualización usando objetos y archivos sintéticos. No abre la biblioteca de pases, no lee ni modifica tarjetas bancarias y no solicita permisos privados de Wallet. `Wallet_animation_proven` permanece en `false`.

## Estado comprobado

- La revisión v11 compiló con xtool y se ejecutó en un dispositivo físico. Su informe pasó el validador estricto: fuente del shader conservada, controles sintéticos correctos y vista nativa rechazada. La traza registró `liveRenderType`, `displayProfile` y `liveRenderedImageSet`. Esto justifica medir el perfil; no demuestra que su ausencia cause el rechazo.
- La revisión v12 compila con código 0 desde Ubuntu 24.04 en WSL2, Swift 6.4.0, xtool 1.22.2 y SDK iPhoneOS 27.0 extraído de Xcode 27. Su validador pasa 26 pruebas. Ejecución física v12 pendiente.
- La revisión v12 observa si existe un perfil, compara perfiles construidos con los mismos metadatos públicos sintéticos y verifica las firmas del runtime. Si el perfil inicial está ausente y los controles coinciden, vincula un perfil propio, comprueba su lectura posterior y repite el intento de crear la vista. No asigna estilos ni enums bancarios supuestos.
- La revisi�n v13 se ejecut�: captura acotada del constructor aceptada por el validador; contiene ramas seg�n el tipo derivado del pase, pero 512 bytes no cubren la rama usada. La vista sigue devolviendo nil.
- La revisi�n v14 verifica la firma y el m�dulo del constructor `PKLiveRenderedCardFaceView initWithFrame:pass:`. Lee exactamente 4.096 bytes de ese c�digo usando `vm_read_overwrite(mach_task_self(), ...)`, sin obtener puertos de otros procesos ni modificar instrucciones. Reconoce �nicamente stubs ARM64 est�ndar de selectores Objective-C y exporta como m�ximo 128 nombres con sus desplazamientos dentro de la ventana de c�digo. Comprueba que los nombres son selectores registrados. No exporta direcciones ni rutas. La captura no equivale a una vista creada ni a ejecuci�n de shaders.
- `source-parity.json` documenta los hashes de los siete archivos Objective-C de la revisión preparada. `Probe.m` conserva los bytes de su `main.m`, renombrado para evitar que SwiftPM lo trate como un objetivo ejecutable.

## Preparar el SDK y compilar

Con Swift 6.4 y xtool instalados en Linux:

```sh
xtool sdk install "$XCODE_XIP" --slim
swift sdk list
xtool dev build --configuration release --ipa
python3 -m unittest discover -s tests -v
```

`XCODE_XIP` debe contener la ruta Linux al archivo obtenido desde Apple. Conviene compilar en el sistema de archivos Linux de WSL. El SDK instalado aparece como `darwin`. El proyecto biblioteca se enlaza con el ejecutable generado por xtool y aporta su función `main`.

No añadir `--sign`: esta compilación no necesita credenciales de Apple en xtool. El propietario puede firmar e instalar con Sideloadly, introduciendo personalmente sus credenciales cuando corresponda.

## Paquete v14

`xtool/ShaderDiagnostics.ipa` sin firma: 140.607 bytes, SHA-256 `6f4df255ed04978381ab2702b80a8df925ac24f14d670338e06528d8a5ee6b03`. ZIP íntegro, ejecutable Mach-O arm64, mínimo iOS 17.0, versión 1.12 (12), identificador `local.walletstudio.shaderdiagnostics`. La firma posterior cambia el hash y puede cambiar el identificador.

Abrir la app, esperar a «Shaders v12 · diagnóstico guardado» y recuperar su propio `Documents/shader-report.json`. Validar el informe antes de extraer conclusiones. Una vista o shader en esta app no prueba animación dentro de Apple Wallet. No publicar informes del dispositivo ni identificadores de firma.

La compilación v11 anterior permanece en el historial del repositorio; su IPA sin firma tenía SHA-256 `70cae006bffee0b55a321ae5a1c34b4ebf1e9e6da545729e2f24ad816cdbaa23`.

## Fuentes

- https://github.com/xtool-org/xtool/tree/1.22.2
- https://xtool.sh/documentation/xtool/installation-linux/
- `Sources/PackLib/Packer.swift`: genera un ejecutable C que depende de un producto biblioteca.
- `Sources/XToolSupport/DevCommand.swift`: permite `build --ipa` sin credenciales de firma.
- Declaración de referencia de `PKPassDisplayProfile`; cada firma usada se vuelve a comprobar en el runtime: https://github.com/MTACS/iOS-17-Runtime-Headers/blob/d1d960dfaa4107765dd7fcf891e4967c0930d5fd/PrivateFrameworks/PassKitCore.framework/PKPassDisplayProfile.h
