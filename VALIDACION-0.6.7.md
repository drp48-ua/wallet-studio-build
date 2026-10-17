# Validación de Wallet Studio 0.6.7

Fuente compilada: `68bc5e11c3467eb748f32241033ebc0f92459558`, sobre la versión 0.6.6. La investigación de GIF permanece fuera de esta entrega.

## Comprobaciones automatizadas

[Windows CI, ejecución 39018484547](https://github.com/drp48-ua/wallet-studio-build/actions/runs/39018484547): todas las fases terminaron correctamente el 17 de octubre de 2026.

- Rust: 155 pruebas aprobadas, una ignorada y cuatro pruebas de hardware excluidas según AGENTS.md. Prueba adicional del motor: una aprobada.
- Servicio de cuentas: 49 pruebas, una omitida por requerir PostgreSQL sintético externo, ninguna fallida.
- Integración de escritorio con dos perfiles sintéticos: una prueba aprobada.
- Compilación Windows MSVC, recurso de versión, empaquetado y carga de artefacto: correctos.
- Siete pruebas nuevas cubren ejecución concurrente real de miniaturas, límites entre cuentas, reintentos, selección, preparación del diseño, reparación de caché y conservación/rechazo de sesión cifrada.

La compilación local inicial fue bloqueada por Control de aplicaciones de Windows, error 4551, al ejecutar un build-script de zerocopy. No se modificó ni eludió esa protección; el candidato se compiló mediante el workflow Windows existente en GitHub.

## Revisión visual

Se abrió el ejecutable compilado en una demostración aislada, sin cuenta real ni teléfono, con 50 PNG sintéticos. Se revisaron ventana de 960×640 y ventana maximizada de 1920 píxeles de ancho.

- Sin selección, el único botón de uso aparece deshabilitado.
- Seleccionar una miniatura muestra una previsualización mayor, identifica la selección y habilita la acción compartida.
- Las filas visibles del catálogo cargan imágenes sin botones repetidos ni saltos de tamaño. Se conservan las imágenes al volver a Biblioteca.
- Se seleccionó el diseño sintético «demo 27». Tras pulsar el botón, la app mostró preparación y abrió Tarjetas con esa misma imagen visible en «Nuevo diseño».
- La demostración mantuvo deshabilitada la aplicación al teléfono.

La sesión cifrada, los fallos temporales y el rechazo del token se verificaron con datos sintéticos en las pruebas. No se inició sesión con una cuenta de cliente ni se realizó una transferencia física durante esta aceptación.

## Integridad del paquete

- ZIP: `Wallet-Studio-0.6.7-Windows.zip`, 8.593.360 bytes. SHA-256: `bd751997a812cb71c3c0ef5c2268dc99cabdf205213078f882886e71026dac61`.
- EXE: versión de archivo y producto 0.6.7. SHA-256: `ccda2084eb66df885ed1de051aa71a397ed2879b2566c784ba18919cc3c41536`.
- CRC de todas las entradas del ZIP correcto; ejecutable empaquetado idéntico al revisado. Tamaño, versión y hash del manifiesto coinciden.
- Se verificó la exclusión de los nombres y hashes de los 50 diseños comerciales y de carpetas de datos personales. La carpeta `designs` contiene únicamente README.
- Estado Authenticode: `NotSigned`. No se publicó una actualización automática ni se modificó producción.

El ZIP y una copia del EXE se entregaron en Descargas. No se reemplazó ninguna instalación anterior ni se modificaron sus originales o sesiones.
