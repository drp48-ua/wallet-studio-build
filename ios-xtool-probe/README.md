# Diagnóstico sintético con xtool

App separada para investigar shaders y renderizado mediante objetos y archivos sintéticos. No abre la biblioteca de pases, no lee ni modifica tarjetas bancarias y no solicita permisos privados de Wallet. `Wallet_animation_proven` permanece en `false`.

## Resultado físico comprobado

v17 crea una vista nativa `PKTexturedCardView`. El intento inicial con solo mapa de color devuelve nil. Al añadir mapas propios de metal y normales, los tres CGImage están disponibles y la vista candidata se crea. El control rojo se captura como (255,0,0), alfa 255. Las seis muestras permanecen grises (115,115,115): no se observan las tres fases del fragment shader candidato. Una vista aceptada no prueba animación ni integración con Apple Wallet.

v11-v14 cargaron textos propios pero devolvieron nil al construir la vista. v12 comprobó que el perfil ya existía. v13-v14 capturaron código acotado del constructor dentro del propio proceso. La rama correspondiente al tipo derivado 1 consume mapas de color, metal y normales y llama a `initWithDiffuse:metalness:normal:`. Los nombres resueltos son referencias estáticas, no una traza de ejecución. Una ventana de código puede incluir métodos contiguos.

v15 falló dos veces en el control de captura, antes de probar los mapas completos. v16 registró rojo (163,0,0) con aplicación activa y ventana presente. v17 fijó rango estándar, escala 1, salida opaca y conversión sRGB. Mantiene el umbral rojo y exige alfa 255. Los resultados fallidos anteriores no acreditan la prueba de materiales.

## Revisión en preparación

v18 mantiene la prueba de materiales y se ejecutó: vuelve a crear la vista, con seis muestras grises. El inventario de métodos de `PKTexturedCardView` expone render loop, pausa y movimiento. Esa clase no implementa el constructor con tres mapas buscado, por lo que su captura de código queda explícitamente desconocida.

v19 repitió la vista estática, pero detuvo el catálogo al superar el límite preventivo de clases del proceso. Ese catálogo vacío no acredita ausencia de clases gráficas.

v20 usa `objc_copyClassNamesForImage` para consultar directamente PassKitUI y PassKitCore. Busca la clase propietaria del constructor observado, comprobando la firma antes de capturar su código. Añade un catálogo de hasta 16 clases relacionadas con renderizado, con hasta ocho métodos relevantes por clase y banderas de truncamiento. Solo consulta metadatos del runtime; no instancia servicios ni modelos de tarjetas. Compila y pasan 53 pruebas; su ejecución física está pendiente.

La lectura usa `vm_read_overwrite(mach_task_self(), ...)`, únicamente para implementaciones concretas de PassKitUI o PassKitCore cargadas en esta app. Nunca obtiene puertos de otros procesos, modifica instrucciones ni exporta direcciones. Reconoce stubs ARM64 estándar y limita la salida a 4.096 bytes por implementación y 128 selectores con desplazamientos. El inventario no contiene valores de propiedades de pases.

## Compilación

Entorno comprobado: Ubuntu 24.04 en WSL2, Swift 6.4.0, xtool 1.22.2 y SDK iPhoneOS 27.0 obtenido de Xcode 27.

```sh
xtool sdk install "$XCODE_XIP" --slim
swift sdk list
xtool dev build --configuration release --ipa
python3 -m unittest discover -s tests -v
```

Compilar en el sistema de archivos Linux. El SDK aparece como `darwin`. El producto biblioteca aporta `main` al ejecutable generado por xtool. `source-parity.json` conserva los hashes de la instantánea local de los ocho archivos Objective-C; `Probe.m` corresponde a `main.m` renombrado para SwiftPM.

No añadir `--sign`: la compilación no necesita credenciales Apple en xtool. El propietario firma con Sideloadly e introduce personalmente sus credenciales. La firma cambia el hash y puede cambiar el identificador.

Paquete arm64, mínimo iOS 17.0, identificador original `local.walletstudio.shaderdiagnostics`. La app escribe su propio `Documents/shader-report.json`. Validar el informe antes de interpretar resultados. No publicar informes del dispositivo, código Apple capturado ni identificadores de firma.

## Fuentes

- https://github.com/xtool-org/xtool/tree/1.22.2
- https://xtool.sh/documentation/xtool/installation-linux/
- `Sources/PackLib/Packer.swift`: genera un ejecutable que depende de un producto biblioteca.
- `Sources/XToolSupport/DevCommand.swift`: permite `build --ipa` sin credenciales de firma.
- Declaración del perfil, con firmas comprobadas en el runtime: https://github.com/MTACS/iOS-17-Runtime-Headers/blob/d1d960dfaa4107765dd7fcf891e4967c0930d5fd/PrivateFrameworks/PassKitCore.framework/PKPassDisplayProfile.h
