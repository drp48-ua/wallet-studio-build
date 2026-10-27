# Wallet Studio 0.6.24: preparación de distribución y actualizaciones

## Solicitud y compatibilidad

El usuario confirmó que probó satisfactoriamente todas las funciones en su iPhone después de la migración. Es una prueba física reportada por el usuario sobre ese dispositivo y 0.6.23; no certifica todos los modelos, versiones de iOS, Windows o equipos. La app Windows requiere Windows de 64 bits, componentes Apple compatibles, USB y autorización Confiar en el iPhone. Sus originales y la identidad de cada dispositivo siguen aislados y protegidos; esta entrega no cambia captura ni recuperación.

El paquete contiene la app y sus licencias/documentación, sin originales, credenciales ni diseños privados. La biblioteca se obtiene con una cuenta autorizada. Los componentes Apple no se redistribuyen: la app los comprueba antes del inicio de sesión y ofrece descargar el instalador oficial de iTunes mediante un clic. Lo verifica con Authenticode y editor Apple antes de ofrecer instalar; no instala, eleva permisos ni cambia el teléfono silenciosamente.

## Cambios

- Detección de componentes Apple mediante carpetas oficiales Common Files de Windows, sin fijar la unidad C ni cargar DLL desde variables arbitrarias.
- Orientaciones de inicio distinguen instalación ausente de servicio detenido o no consultable. Un instalador en caché inválido queda conservado antes de volver a descargar tras el clic.
- Ayuda elimina el acceso a GitHub y conserva Buscar actualizaciones; la demostración no consulta el canal.
- Compilación MSVC con CRT estática para todas las dependencias y comprobación de importaciones externas VCRUNTIME/MSVCP/CONCRT. La prueba MSVC debe realizarse en CI; un build GNU local no sustituye esa evidencia.
- Las versiones nuevas prevalecen sobre números de build locales antiguos; dentro de la misma versión se conserva la protección frente a retrocesos y redefinición de publicaciones.

## Evidencia disponible

Un único implementador crack_builder configurado preparó los cambios; el responsable revisó el diff y detectó que una sustitución Unicode inicial no había retirado el botón GitHub, que se corrigió antes de distribuir.

Suite nativa GNU en serie: 239 pruebas correctas, cero fallidas, siete ignoradas y cuatro exclusiones de hardware. Tras retirar las tres líneas de Ayuda: recompilación correcta y 16 pruebas app correctas. Focos: cuatro Apple correctas (una ignorada) y veinte de actualizaciones correctas. El responsable ejecutó además la prueba ignorada explícita de firma Apple ya instalada: una correcta, sin ejecutar el programa Apple. Se comprobó una muestra de la descarga oficial, con redirección únicamente de www.apple.com a secure-appldnld.apple.com, HTTP 206 y cabecera PE válida; no se confunde esa muestra con descargar/verificar un instalador entero.

Los errores de codificación UTF-8 inicial y del primer runner no se presentaron como pruebas correctas; los registros finales contienen resultados reales del binario. Todos los logs, fixtures y herramientas auxiliares permanecen ignorados en server/test-runtime.

## Canal de publicación: preparado, todavía pendiente de integración

El main remoto solo contenía tres commits; la rama actual estaba 71 commits por delante y no contenía cambios exclusivos de main. El workflow desktop-release.yml existe en esta rama y no estaba integrado en main. Se comprobaron en GitHub permisos de administrador, repositorio público y ausencia inicial de secretos/variables de Actions.

El usuario autorizó explícitamente guardar la clave existente de firma como secreto cifrado de GitHub Actions y habilitar la publicación automática. La clave fue comprobada localmente contra la clave pública fijada del cliente, sin imprimirla. Se guardó como WALLET_STUDIO_UPDATE_SIGNING_KEY; se configuraron WALLET_STUDIO_RELEASE_APPROVED=true y WALLET_STUDIO_CLOUD_URL con el servicio oficial. Nunca se añadió la clave a Git.

Antes de afirmar que las actualizaciones desde main funcionan: terminar el paquete 0.6.24, revisar UI pequeña/grande, validar CI MSVC, integrar los cambios mediante un PR y comprobar la primera publicación firmada y su aceptación por el cliente. No sobrescribir main ni usar force-push. El build local 0.6.24 se prepara con canal habilitado y build_id=0 para aceptar la primera publicación CI de esa misma versión; no usar IDs de fecha grandes para builds locales.

## Aceptación y publicación, 27 de octubre

Paquete local terminado y auditado: 676 archivos, sin originales privados, sesiones ni claves. EXE SHA-256: 2f1c09c19dd712ebfda7b1bb3c263da034ac04ae639da55e5f5e09cb68313870. ZIP SHA-256: cebd9b1f8db4fd298157dcd08ac4dc3c35e7916c3f881d42eeb2308ec3eaf54a. Entrega local: C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.24-Windows.zip.

Las dos comprobaciones Windows/MSVC del commit a28cd93aa7fa36acf067cc967c773eb48bfa1148 terminaron correctamente: ejecuciones 40489445613 y 40489529006. Ayuda revisada en demostración aislada WSDEMO96.exe, con tres tarjetas sintéticas, sin consultas de red ni USB: vista pequeña con desplazamiento y ventana maximizada. No hay enlace a GitHub. Una primera ventana de nombre largo mostraba el perfil normal y no se considera prueba aislada. El permiso para abrir otra copia grande caducó; se amplió la ventana de prueba ya autorizada.

PR #7 integrado mediante merge normal, sin force-push ni reescritura: https://github.com/drp48-ua/wallet-studio-build/pull/7. Main resultante: aca5756ff372bca6a040e9ee9cbc2418c20e4d66. La publicación automática autorizada empezó en https://github.com/drp48-ua/wallet-studio-build/actions/runs/40494748264, build 1. Todavía debe comprobarse su resultado, firma y aceptación del canal real. El paquete local lleva build_id=0 y puede aceptar ese build 1 de la misma versión.

El servicio Supabase/R2 migrado sigue activo y no se cambia para esta entrega. No se han realizado nuevas escrituras en el iPhone. No se ha probado un PC limpio sin componentes Apple ni se certifica compatibilidad universal.

## Resultado final del canal

Publicación 40494748264 terminada correctamente en 14m54s. Release estable: https://github.com/drp48-ua/wallet-studio-build/releases/tag/desktop-build-1. Versión 0.6.24, build 1, commit main aca5756ff372bca6a040e9ee9cbc2418c20e4d66. Se descargaron los assets reales y se verificó RSA-PSS/SHA-256 con la clave pública fijada del cliente, longitud y SHA-256 del ZIP, SHA-256 del EXE y 676 archivos sin rutas privadas/prohibidas.

ZIP publicado SHA-256: 02fa8e2d1d5bb4e34a4bd669ecae9725063d0918f6ba68586be3e5435143381b. EXE MSVC publicado SHA-256: 0467a8853d407247201cc96f1a5fc545d8bd9d3226d777ee6d4a8625b4f3ec40. Copia de entrega: C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.24-Windows-publicada.zip. El ZIP GNU anterior se conserva separado.

En la app normal que ya estaba abierta, Buscar actualizaciones pasó de canal no disponible a «Wallet Studio está actualizado» con el canal real publicado. El código verifica firma y metadatos antes de emitir ese resultado. No se presenta ese mensaje como prueba de una sustitución/reinicio: no se ha forzado un reinicio mientras la app comprobaba una copia. Las pruebas del actualizador cubren preparación y protección ante retroceso; la publicación completa demuestra el flujo main, pruebas, firma y assets reales. Los cambios futuros en main publicarán builds nuevos solo si superan el workflow, y el cliente los aplicará al volver a abrirse.
