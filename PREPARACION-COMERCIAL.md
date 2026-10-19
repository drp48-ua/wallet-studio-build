# Preparación comercial Windows — 17/10/2026

## Resultado y alcance

Copia independiente de `drp48-ua/wallet-studio-build`, rama `codex/preparacion-comercial-windows`, basada en `66c6dcd`. No se han enviado cambios ni publicado binarios. La carpeta del proyecto original y sus cambios de animación no se han tocado.

**Todavía no hay paquete listo para vender.** Esta entrega prepara controles de distribución y documenta bloqueos; no certifica una compilación Windows estable ni implementa licencias.

La versión real de partida es **0.5.3**: coincide en el último commit, las notas, update_info.rs y los recursos Windows. El 1.2.4 de Cargo era una inconsistencia heredada. Cargo.toml y la entrada raíz de Cargo.lock se alinean a 0.5.3; la interfaz toma la versión de Cargo y los scripts rechazan divergencias. No se actualizan dependencias. Antes de publicar una revisión distinta de un binario ya entregado, asigna una versión nueva y actualiza también los recursos Windows; no reutilices una versión comercial para archivos diferentes.

## Correcciones realizadas

- Eliminada la subida de artefactos Windows en el workflow público. Los cambios son locales: el workflow remoto sigue como estaba hasta que se autorice su publicación. No se han consultado ni retirado artefactos históricos.
- Retirado el destino público de actualizaciones de la aplicación. El panel informa de entrega manual y soporte. No se reemplazan ejecutables automáticamente.
- Mensajes de dependencias Apple en español, con fuente oficial y acciones concretas, conservando el mismo cargador, rutas y protocolo.
- Diagnóstico separado para requisitos del cliente y herramientas de compilación. Se detecta Rust estable MSVC x64, Cargo, compilador/enlazador y rc.exe.
- Paquete con lista explícita de archivos; no incluye informes históricos de teléfono, datos personales ni todos los archivos de designs indiscriminadamente. Rechaza rutas relativas ascendentes y enlaces. Los diseños requieren nombre aprobado, SHA-256 y aviso de derechos.
- Licencias conservadas. Auditoría de cobertura contra Cargo.lock que impide un paquete definitivo con avisos ausentes. Firma Authenticode exigida por defecto; -Draft solo genera una revisión interna marcada NO-VENDER.
- Hash por archivo, versión/plataforma, manifiesto de entrega, SHA-256 del ZIP y lectura posterior del archivo comprimido. Verificador manual exige versión posterior, integridad y editor esperado antes de instalar; no sustituye archivos.
- Manual de instalación y uso, actualizaciones, soporte y condiciones previstas. Sin cambios en neumorfismo, GIF, baseline, transferencia o recuperación.

## Dependencias y validación local

No están disponibles Rust/rustup/Cargo, MSVC (`cl.exe`, `link.exe`) ni Windows SDK (`rc.exe`). Tampoco aparecen las tres DLL de Apple en las rutas del cargador, servicio Apple ni aplicación Apple registrada en las comprobaciones realizadas. No se instalaron herramientas ni controladores.

La detección de archivos no prueba arquitectura ni carga; la app carga las bibliotecas. Debe comprobarse con una instalación oficial soportada de Apple. No se descarga ni redistribuye ninguna DLL Apple.

Orden de comprobación:

1. Revisión estática de versiones, scripts, workflow, distribución, dependencias y licencias; sin cambios de dependencias. No equivale a una auditoría de vulnerabilidades de crates.
2. `./scripts/test-distribution.ps1`: 12 comprobaciones correctas con archivos sintéticos. Comprueban exclusión de diseños personales, rutas ascendentes/absolutas, uniones de directorio, diseños alterados, ausencia de avisos, incoherencia de versión, retroceso de versión, hash incorrecto y ausencia de subida de artefactos Windows. También se analizó la sintaxis de todos los scripts.
3. Pruebas Rust y compilación Windows: bloqueadas por herramientas ausentes. No se ha generado un EXE nuevo. La comprobación visual de mensajes/panel con tamaños pequeño y grande y estados vacío, desconectado, conectado y controles habilitados/deshabilitados queda pendiente.
4. Paquete: el definitivo queda bloqueado por licencias; el borrador no puede crearse sin EXE. Las verificaciones sobre un ZIP comercial real, Authenticode válido y carga de DLL siguen pendientes.

No se han probado USB, modificación de Wallet ni restauración física. Se requiere un iPhone compatible y autorización humana antes de realizar esas pruebas.

## Licencias de terceros y diseños

El índice existente tiene 62 entradas sin avisos. La comparación con Cargo.lock detecta **64 huecos**, porque `color_quant 1.1.0` y `gif 0.14.2` tampoco están indexados. Incluye dependencias de otras plataformas: el control es conservador para todo el lockfile, no afirma que las 64 se enlacen en Windows.

Para listar los pendientes:

```powershell
. ./scripts/distribution-common.ps1
Get-LicenseGaps $PWD.Path
```

No basta con rellenar una etiqueta SPDX. Se deben recuperar los textos y atribuciones originales de la versión concreta, añadirlos al índice y comprobar los avisos adicionales (por ejemplo fuentes). Una consulta puntual al archivo original de eframe 0.33.3 confirmó que no contiene LICENSE/COPYING/NOTICE, por lo que parte del trabajo requiere verificar el repositorio original y su revisión exacta. No se han inventado textos ni copiado licencias de otras versiones. El inventario completo y su resolución quedan pendientes; no se inició una investigación amplia de 64 dependencias.

`distribution/designs.json` está vacío. No se trasladaron imágenes de la otra carpeta ni diseños privados. Para aprobar una imagen, añade una entrada con `file` (nombre simple), `sha256`, `rights` (procedencia/permiso verificable) y `licenseFile` (archivo en `distribution/design-licenses/`). La imagen vive en windows/designs. El paquete conserva su aviso. Una lista vacía entrega una biblioteca vacía; hay que decidir si esa es la oferta comercial o aportar diseños autorizados. El GIF de pruebas y su licencia siguen intactos junto al código.

El código se distribuye bajo MIT y sus avisos se conservan: el texto ya concede derechos de copia, modificación, redistribución y venta. No se debe presentar el límite comercial de dos equipos como una revocación de esos derechos. Al ser público el código, un tercero puede recompilarlo; una activación no constituye protección absoluta. Hay que definir qué cubre la compra (servicio de activación, entregas oficiales, soporte y actualizaciones) y revisar los términos antes de vender.

## Propuesta mínima de licencia — requiere autorización antes de implementar

Condiciones confirmadas: **6,99 €, pago único, 2 ordenadores Windows, actualizaciones incluidas indefinidamente y sin suscripción**. Soporte: javicarmelosomanager@gmail.com.

Una licencia local editable o un contador en el EXE no controla dos equipos. Propuesta: una API HTTPS pequeña, base de datos de licencias/activaciones y almacenamiento privado de paquetes. Por ejemplo, Cloudflare Workers + D1 y almacenamiento privado. No se ha creado cuenta, servidor ni contratado servicio alguno.

- Registrar la compra en el servidor; al principio puede hacerse manualmente desde un panel administrativo protegido con MFA. El ejecutable no necesita integración Shopify. Automatizar pedidos más adelante exigiría verificar la firma del webhook e idempotencia en el servidor.
- Emitir una clave aleatoria de alta entropía; guardar su hash, referencia de pedido y estado del derecho de uso. No guardar datos bancarios ni datos del iPhone.
- Cada instalación genera una clave de dispositivo protegida por Windows. El servidor valida una prueba de posesión, asigna la plaza de forma transaccional y limita a dos activaciones simultáneas, con índice único e idempotencia. Una huella o archivo local no será la autoridad del contador.
- Devolver una autorización firmada ligada a esa clave. Solo la clave pública verificadora va en el cliente. Mantener las claves privadas en secretos del servidor o servicio de firma; rotarlas con un procedimiento documentado.
- Propuesta a decidir: permiso sin conexión de 30 días, renovado automáticamente, con aviso previo. La compra no caduca; caduca el comprobante temporal. Una plaza revocada puede seguir funcionando hasta vencer ese permiso: reducir el plazo mejora el control y aumenta la dependencia del servicio.
- Cambio de ordenador: desactivar uno desde el portal y activar el nuevo. Reinstalación o equipo averiado: recuperación mediante correo verificado y pedido, liberando la plaza anterior; límites antiabuso y atención manual. No usar exclusivamente MachineGuid ni identificar un equipo por sus tarjetas.
- Recuperación segura: una licencia caducada, un fallo de servidor o una revocación no deben impedir consultar evidencia o restaurar originales ya guardados. El control de activación se aplicaría al inicio de nuevas personalizaciones, manteniendo accesibles los mecanismos de recuperación.
- MFA para administración, límites de intentos y peticiones, secretos fuera de logs, copias de seguridad probadas, revisiones de dependencias, política de privacidad/retención y proceso de incidentes. No afirmar que impide parches a un cliente de código abierto.

Coste orientativo verificado: Workers Paid parte de **5 USD/mes**; bases de datos, almacenamiento, tráfico, correo, dominio, impuestos y firma de código pueden añadir coste. El nivel gratuito permite prototipos dentro de sus límites; no es una garantía de servicio indefinido. Debe presupuestarse el mantenimiento, la recuperación de plazas y la disponibilidad del servicio frente a una compra única de 6,99 €. Fuente: [precios oficiales Workers](https://developers.cloudflare.com/workers/platform/pricing/). No se eligió ni compró un certificado de firma.

Alternativa de lanzamiento: términos de dos equipos sin control técnico, claramente declarados. No cumple un requisito de activación exigible; requiere una decisión comercial expresa.

## Actualizaciones autorizadas

Fase manual: entregar versiones y sumas por el canal de compra de Digital Products, con firma Authenticode y comprobador local. Debe verificarse la configuración real de la aplicación instalada y cómo recuperan acceso los compradores; no se ha accedido a Shopify ni confirmado que sus enlaces sean personales, revocables o que no se puedan compartir.

Fase con servidor: autenticar al comprador/instalación, comprobar el derecho indefinido a actualizaciones y emitir un enlace de corta duración al paquete privado. Un enlace temporal puede compartirse mientras vive: limitar plazo, controlar acceso y evitar repositorios públicos. Firmar un manifiesto que fije versión, plataforma, tamaño, hash y versión mínima de recuperación. El cliente comprueba firma, versión monotónica y hash antes de una sustitución atómica con la app cerrada. Conservar una versión compatible y toda la evidencia; nunca borrar datos del perfil durante una actualización. No incrustar tokens Shopify, GitHub ni claves de firma en el EXE.

## Próximos pasos y entrega a Shopify

1. Completar avisos de terceros y decidir los diseños incluidos.
2. Preparar Rust estable + MSVC/SDK en un entorno Windows. Ejecutar `./scripts/test-distribution.ps1`, `./scripts/build-windows.ps1 -TestOnly` y después `./scripts/build-windows.ps1` una vez.
3. Revisar visualmente la app y autorizar pruebas físicas por separado. Comprobar dependencias del EXE en un Windows limpio; no confundir compilación con USB validado.
4. Decidir y autorizar el servidor mínimo, política sin conexión y recuperación. Aprobar términos que respeten MIT y las condiciones comerciales; no vender el control de dos equipos como implementado.
5. Firmar el EXE con un certificado de editor confiable y ejecutar `./scripts/package-windows.ps1`. El borrador `-Draft` solo sirve para revisión interna.
6. Revisar el ZIP resultante, versión, firma y hashes. Subir manualmente a **Shopify Digital Products** el `Wallet-Studio-<versión>-Windows.zip` definitivo, con `SHA256SUMS.txt` y los datos de versión/editor del `release-manifest.json` accesibles por el canal de compra. No subir el repositorio, dist completo, la carpeta stage, datos del perfil, diseños personales ni un BORRADOR-NO-VENDER.
7. Hacer una compra de prueba autorizada y comprobar entrega/recuperación de acceso y actualización de un comprador anterior. No se ha ejecutado esa compra.

Decisiones pendientes: control técnico de activaciones y su coste; plazo sin conexión/revocación; certificado de firma; diseños autorizados o biblioteca vacía; configuración de entrega y derechos de actualización; número de versión final si 0.5.3 ya se entregó; términos comerciales compatibles con las licencias existentes. La autorización de backend se debe solicitar antes de comenzar esa implementación.
