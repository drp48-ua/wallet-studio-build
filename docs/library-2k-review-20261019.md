# Biblioteca nativa 2K: despliegue y aceptación

El catálogo nativo contiene 56 PNG de 1536×969, con un máximo de 4 MiB por archivo.
Las imágenes son privadas: no se incluyen en Git, instaladores ni URLs públicas.

## Contrato

- `server/design-catalog.json`: metadatos nativos, SHA-256 y bytes reales.
- `server/design-catalog.legacy.json`: 56 entradas históricas, sin modificar los objetos existentes.
- Sin cabecera/capacidad desconocida: 50 entradas legacy. `expanded-v1` y
  `native-2k-v1`: 56 legacy; clientes v1 distribuidos no enviaban cabecera al descargar.
- `X-Wallet-Studio-Catalog: native-2k-v2` (cliente 0.6.10):
  56 nativas, tanto catálogo como descarga. v2 recibe metadatos del original
  sin miniatura separada; el cliente admite ese formato y usa el original.
- Objeto nativo inmutable: `studio-50-v1/native/{id}-{sha256}.png`.
- `WALLET_DESIGN_CATALOG` configura el nativo; `WALLET_DESIGN_LEGACY_CATALOG` configura el legacy independientemente.

## Preparación offline

```powershell
python server/upload_designs.py --source <carpeta-56-PNG-revisados> --manifest-only
python -m unittest discover -s server/tests -p "test_*.py"
```

El modo `--manifest-only` exige los 56 archivos, valida PNG/dimensiones/bytes y sustituye
el manifiesto atómicamente. No conecta con Supabase ni requiere credenciales.

## Publicación: requiere autoridad y acceso al entorno

1. Comprobar que el bucket `wallet-designs` sigue privado y permite PNG de **4 MiB**.
   Los buckets antiguos pueden conservar un límite de 1 MiB. El aprovisionador rechaza
   ese límite antes de subir archivos; no cambia permisos ni límites silenciosamente.
2. Aprovisionar los 56 objetos content-addressed y verificar su lectura de vuelta
   con el manifiesto final. Usar el mecanismo seguro de credenciales del entorno,
   nunca pegar claves en conversación ni registrarlas en logs.
3. Solo después desplegar juntos servidor, catálogo nativo y catálogo legacy.
4. Compilar el cliente con el workflow Windows/MSVC oficial y ejecutar las pruebas
   de `AGENTS.md`. Las comprobaciones Python no sustituyen esta compilación.
5. Cerrar la aplicación, retirar **solo** la caché `purchased` con copia recuperable,
   preservando `own`, sesiones y copias bancarias. Abrir el nuevo cliente.
6. Con una cuenta autorizada, verificar en la Biblioteca las 56 vistas sin errores,
   su vista ampliada, tamaños de ventana pequeño/grande y escalado DPI.

El cliente 0.6.10 acepta un catálogo v2 sin miniatura separada: en ese caso utiliza
el original tanto en Biblioteca como al aplicar el diseño. El servidor conserva
las miniaturas legacy para clientes anteriores y peticiones pendientes anteriores
a la actualización del catálogo. El cliente valida dimensiones, bytes y SHA-256.

## Publicación autorizada del 20 de octubre

Los 56 originales se cargaron en el bucket privado, bajo `studio-50-v1/native/`,
con nombres content-addressed. El despliegue `6bc7094` devolvió 56 originales
nativos y verificó ws-001, ws-026, ws-030 y ws-056 a 1536 × 969 desde el cliente
oficial 0.6.10. La prueba masiva posterior detectó errores HTTP 502, por lo que
esa muestra inicial no constituye aceptación completa.

Se corrigió además la retención de trabajadores por conexiones HTTP keep-alive:
las respuestas cierran la conexión y liberan la plaza al terminar. La comprobación
de salud responde durante una descarga, con el mismo límite de lectura de 8192
bytes/0,2 segundos y sin crear trabajadores adicionales. Las rutas protegidas
mantienen 503 y Retry-After cuando no hay capacidad. No cambian los controles
de acceso, las licencias ni la privacidad del bucket.

## Evidencia y límites

Las pruebas HTTP usan cuentas y PNG sintéticos, lector real del catálogo y bucket
simulado privado. Verifican los 56 assets nativos y los 56 legacy por separado;
no demuestran que producción esté desplegada ni que la ventana real esté corregida.
Una demo aislada tampoco constituye prueba del acceso o almacenamiento en producción.
