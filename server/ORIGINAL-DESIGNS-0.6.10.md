# Originales y miniaturas — 0.6.10

La falta de resolución y las vistas no disponibles son problemas distintos. El servidor anterior validaba y transmitía bytes PNG exactos: no comprimía ni redimensionaba imágenes. Los 56 registros existentes apuntaban a archivos de 320×200; el proceso privado de preparación había reducido los primeros 50 originales a ese tamaño. La compresión PNG es sin pérdida; reducir píxeles sí elimina detalle.

## Contrato publicado

Se conserva cada archivo legado y su metadata plana. El campo opcional `native` contiene únicamente `filename` (mismo ID.png), `sha256`, `bytes`, `width` y `height` reales del original recortado. Sus objetos permanecen privados en `wallet-designs/studio-50-v1/native-2k-v1/ID.png`.

El nuevo cliente negocia `X-Wallet-Studio-Catalog: native-2k-v2` tanto al listar como al descargar. La versión v1 se conserva sin cambios porque clientes ya distribuidos la enviaban solo al listar. La respuesta v2 incluye metadata del original y `thumbnail` con la metadata legada. `/v1/designs/ID/thumbnail` sigue exigiendo la misma cuenta y derecho; no expone URLs públicas. Las respuestas PNG tienen MIME `image/png`.

Los originales conservan dimensiones reales: ancho 640–4096, alto 400–4096, máximo 16 megapíxeles, PNG RGB/RGBA de 8 bits, máximo 4 MiB. El servidor limita la descompresión a los píxeles declarados y verifica formato/CRC/hash/tamaño/dimensiones. El cliente mantiene sus límites de decodificación y comprueba hash, bytes y dimensiones. No se amplía un archivo de 320×200 para declararlo original.

La cuadrícula usa miniaturas independientes. La vista ampliada utiliza el original cuando está publicado; si no existe, conserva la miniatura y explica la limitación. **Usar diseño** se bloquea si el servidor solo ofrece la miniatura. El archivo de Wallet 1536×969 se renderiza desde el original y el recorte del editor. Un original pequeño no gana detalle al ampliarlo; la interfaz muestra sus dimensiones y esa limitación.

## Disponibilidad

Una prueba de capacidad 1 reprodujo cierres TCP sin respuesta para solicitudes concurrentes. Ahora el servidor devuelve un 503 pequeño con Retry-After; drena como máximo 8 KiB de cabeceras con plazo total de 0,2 segundos para impedir que Windows descarte la respuesta por un reset. No aumenta la capacidad de servicio ni crea threads para rechazos.

Los cuatro workers de caché/decodificación se conservan. Las peticiones de red de diseños se serializan; los GET transitorios tienen tres intentos acotados con espera 0,4/0,8 segundos. Las nuevas vistas previas esperan cuando hay sincronización o preparación de uso en curso. Los reintentos de vistas siguen limitados, con recuperación explícita; no se realiza red ni decodificación dentro del pintado.

## Publicación privada por el lead

1. Conservar los masters originales y recortes sin modificar. Preparar PNG a sus dimensiones reales, sin usar miniaturas como fuente. Los seis añadidos no tienen todavía una fuente original verificada en el workspace; no inventar una resolución mayor.
2. Mantener el bucket privado, PNG-only, límite de 4 MiB. Añadir únicamente metadata `native` verificada al catálogo.
3. Ejecutar `python server/upload_designs.py --source RUTA_PRIVADA --catalog server/design-catalog.json --native --only IDS_VALIDOS`. Las credenciales proceden del entorno; el script verifica todos los seleccionados antes de la primera petición, crea objetos sin sobrescribir y comprueba su descarga exacta. No imprime secretos ni rutas privadas.
4. Desplegar el backend y comprobar acceso sin sesión 401; catálogo antiguo y v1 conservados; v2 original/miniatura con hashes distintos y dimensiones correctas. Publicar la app después de revisar el candidato.

## Diagnóstico de solo lectura

`Wallet Studio.exe --wallet-studio-design-check` se despacha antes de inicializar Apple, migración o dispositivos. Usa internamente la sesión DPAPI existente y hace solo GET del catálogo y cuatro IDs de prueba fijos. No crea caché ni modifica el perfil. Imprime solo conteos, dimensiones y categorías HTTP/integridad; nunca tokens, cabeceras, cuerpos, identidad, rutas del perfil o credenciales.

Pendientes de completar al recibir el artefacto: resultados finales de pruebas, hashes, diagnóstico del backend actual y aceptación de host. El worker no consulta ni escribe el teléfono. No se certifica todavía un resultado físico en Wallet.

## Validación y comandos

- `cargo +stable-x86_64-pc-windows-gnu test --release --locked --offline --manifest-path windows-source/Cargo.toml -- --skip test_usbmux_query --skip test_list_connected_devices --skip test_syslog_service_receive --skip test_saved_cards_purging`: 166 unitarias y 1 de motor aprobadas; dos entradas ignoradas y cuatro omisiones de hardware. La referencia inicial de la prueba sintética usaba estirado completo mientras el editor existente usa recorte centrado; se corrigió la referencia conservando el control negativo de 320×200. No se modificó el render de producción para hacerla pasar.
- `python -m unittest discover -s server/tests -v`: 65 pruebas, tres omisiones opcionales, sin errores tras corregir la entrega TCP del 503 en Windows.
- Tras incorporar el catálogo final del lead: `python -m unittest discover -s server/tests -p test_expanded_catalog.py -v`: seis pruebas, una omisión opcional, sin errores. Las propiedades planas antiguas conservan la misma huella canónica.
- `python server/tests/desktop_integration.py`: integración sintética Windows/WinHTTP/DPAPI; aprobada con el catálogo y código finales.

El entorno utiliza las herramientas GNU existentes de `work/build-tools`, target compartido `work/wallet-studio-build`, temporales del workspace y endpoint oficial compilado. Las pruebas de sockets se ejecutaron fuera del sandbox que bloquea localhost; no se usaron credenciales reales ni dispositivos.

`--wallet-studio-design-check-all` amplía el diagnóstico de solo lectura a todas las miniaturas y originales publicados. Imprime únicamente agregados: solicitudes previstas/intentadas, miniaturas y originales verificados, originales ausentes y categorías de fallos. No inicia nuevas solicitudes después de 240 segundos; la última conserva el límite de WinHTTP. Actualmente el contrato espera 56 miniaturas, 50 originales y seis originales ausentes. Este diagnóstico no concede derechos ni usa una caché como prueba de acceso.

## Recepción final del candidato

Compilación única GNU de producción aprobada en 5m05s. Versión de archivo/producto 0.6.10. Fuente congelada revisada por el lead; ningún cambio de código después del candidato, solo esta recepción.

- EXE `windows/Wallet Studio.exe`: 12.687.872 bytes. SHA-256 `0c58c9e01dec6eb7715de8c303622a30e17b472ef8e6f164e726a5f22ab9f688`.
- ZIP `dist/Wallet-Studio-0.6.10-Windows.zip`: 8.338.651 bytes. SHA-256 `9b801c2c256a80f62cfa83bb257d540efae83cf4b4b6299bd9576cf0985b29af`.
- CRC completo, EXE empaquetado idéntico, manifiesto, LEEME/novedades actuales: aprobados. Los 106 hashes de archivos privados (56 miniaturas y 50 originales) y los datos del perfil están excluidos del ZIP. Diseños empaquetados: únicamente README.
- Recibo privado ignorado: `server/test-runtime/native-original-artifact-receipt.json`. Registro de compilación: `native-original-build.log`.
- Pruebas Rust requeridas: 166 aprobadas, motor 1 aprobado. Integración Windows/DPAPI/nube: 1 aprobada. Servidor: 65 pruebas, 3 omisiones opcionales. Catálogo final: 6 pruebas, 1 omisión opcional. Registros ignorados: `login-storage-required-tests.log`, `login-storage-desktop-integration.log`, `native-original-server-tests.log`, `native-original-final-catalog-tests.log`.

Diagnóstico real previo al despliegue del backend, 19/10/2026: salida 0; el servicio actual devolvió 50 entradas y cero originales negociados con v2. Las tres muestras `ws-001`, `ws-026` y `ws-030` se descargaron secuencialmente y verificaron a 320×200. `ws-056` no figuraba en esa respuesta antigua. Las muestras existen; no se atribuye su fallo visual a pérdida del objeto ni a compresión PNG. El fallo de concurrencia se reprodujo de forma independiente con capacidad 1. Este diagnóstico no verifica todavía las 56 miniaturas ni los 50 originales publicados.

Pendiente del lead: desplegar el backend revisado, completar y verificar la publicación privada, ejecutar el diagnóstico agregado y aceptar la app en host. Seis originales auténticos siguen sin localizarse: sus miniaturas pueden mostrarse, pero no se declaran alta resolución ni se habilita su uso mediante ampliación de 320×200. No se realizaron consultas o transferencias al teléfono por el worker; la mejora física de Wallet sigue sin certificarse. El worker no hizo commit, push ni despliegue.
