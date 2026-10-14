# Continuación: copias, nube, actualizaciones e interfaz

Rama: codex/customer-ready-storage-updates-ui-2026-10-14. Parte del arreglo 0.5.4 (b00e9e2). Main y el trabajo del colaborador no se han sobrescrito.

## Petición

Conservar originales sin interferencias entre instalaciones/versiones, sincronización automática con cuentas y nube, actualizaciones sin pulsar un botón al publicar cambios en main, e interfaz profesional para clientes. El usuario no tiene servidor y autorizó preparar una solución desplegable. Las versiones antiguas necesitan actualización manual inicial; no contienen estos mecanismos de forma retroactiva.

## Implementado en la propuesta

- storage.rs centraliza la raíz de datos, migración create-only, conflictos y demo aislada. Los módulos de persistencia se han adaptado a esa raíz.
- cloud_sync.rs prepara paquetes deterministas de baseline bancario e historial verificable, importa sin reemplazar evidencia y usa sesiones protegidas con DPAPI. La app integra cuenta y tareas de fondo.
- server/app.py ofrece acceso de cuentas y objetos inmutables separados por propietario. SQLite y blobs privados; registro público desactivado por defecto. Dockerfile y compose incluyen el servicio local detrás de un proxy HTTPS pendiente de configurar.
- auto_update.rs consulta un canal fijo de GitHub Releases, valida tamaño/SHA-256/origen/rutas, prepara ejecutables fuera de los datos privados y activa la versión preparada al abrir de nuevo.
- desktop-release.yml propone probar, compilar y publicar paquetes en cada push a main, con número de build incluso si la versión numérica no cambia.
- Interfaz en revisión: cuenta, estado de almacenamiento/sincronización/actualización y refinamientos visuales. Requiere aceptación visual real.

## Validación y límites

Cuatro pruebas del servidor PASARON: aislamiento de propietarios y repetición idempotente, rechazo de accesos/digests incorrectos, rechazo de ZIP con escape de ruta y registro público desactivado. Se corrigieron cierre de SQLite y soporte de rutas largas Windows.

El checkpoint se prepara por cuota compartida baja. Debe comprobarse el resultado final de los tests Windows y revisarse toda la propuesta antes de entregar un ejecutable 0.6.0. La última versión entregada y confirmada por el usuario continúa siendo 0.5.4. No reemplazarla por una compilación parcial.

Pendientes obligatorios:

1. Revisar cambios del worker y ejecutar la suite Windows completa, compilación con recurso de versión 0.6.0 y empaquetado; excluir los cuatro tests de hardware/catálogo privado según AGENTS.md.
2. Probar migración idempotente, duplicados idénticos, conflictos, symlinks y Windows no administrador. Las pruebas deben usar fixtures, nunca originales privados.
3. Probar cliente real contra servidor local con datos sintéticos, dos cuentas y dos perfiles; comprobar que los datos de un usuario nunca se suben a otro y que se importan los originales antes de plantear captura.
4. Cerrar la primera captura de una tarjeta sin copia histórica sincronizada: una respuesta de nube vacía no acredita que el diseño actual sea original. La captura inicial requiere procedencia comprobada y no debe certificar un diseño personalizado.
5. Validar restauración del historial importado y compatibilidad de los esquemas; recuperaciones inciertas permanecen locales y bloqueadas.
6. Añadir/verificar pruebas de actualización completa, ZIP traversal/bombas/duplicados, rechazos de downgrade, caché alterada y relanzamiento. La confianza actual del canal es HTTPS + repositorio fijo + hashes, no firma Authenticode del fabricante.
7. Revisar la app real a 960 y 1180 px, estados vacío/desconectado/conectado y controles habilitados/deshabilitados. El modo --wallet-studio-ui-demo debe aislar datos y evitar USB/nube/actualizaciones.
8. Desplegar servidor HTTPS, volumen cifrado y backup externo; provisionar cuentas. Falta automatizar altas de compradores de Shopify. No hay servicio online activo.
9. Revisar el workflow, proteger main y configurar firma de código antes del lanzamiento general. La publicación está bloqueada hasta configurar la variable de repositorio WALLET_STUDIO_RELEASE_APPROVED=true, después de la aceptación. Integrar solo después de revisión, sin force-push ni sobrescribir main del colaborador.
10. Portar y validar macOS; no afirmar paridad de plataformas.

## Privacidad y cuota

No subir AppData, originales, manifiestos personales, datos bancarios, identificadores, sesiones DPAPI, credenciales ni archivos de runtime del servidor. Los tests son sintéticos. Consultar límites al inicio y en checkpoints; con ≤3 % empezar a guardar, con ≤1 % detener investigación/implementación/tests y cerrar. No gastar el crédito de reinicio sin autorización.
