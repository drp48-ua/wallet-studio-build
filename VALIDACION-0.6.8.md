# Validación 0.6.8 — catálogo ampliado

Código revisado: c275c7f31ab60fbfb2b694fc36a76358279a33b4.
CI Windows: https://github.com/drp48-ua/wallet-studio-build/actions/runs/39024659366

- Rust: 156 pruebas correctas, una ignorada; motor: una correcta.
- Servicio: 55 pruebas correctas, dos omitidas por dependencias externas/activos privados.
- Integración escritorio/servidor con perfiles sintéticos: una correcta.
- Comprobación local adicional de los seis PNG privados contra el catálogo: correcta.
- Los primeros 50 registros del catálogo permanecen intactos. Clientes antiguos reciben 50; 0.6.8 solicita el catálogo ampliado.
- Demo aislada del ejecutable final: versión 0.6.8 y etiqueta «56 DISEÑOS INCLUIDOS» verificadas visualmente. Sin cuentas, red ni teléfono. Demo cerrada al finalizar.
- ZIP: CRC correcto, manifiesto y binario idénticos; imágenes comerciales y datos personales excluidos.

ZIP portable: 8.593.602 bytes.
SHA256 ZIP: 8f47070fcf6327466bb33862e814483b7f1160862c5bfd217e3bf039a51a84e3
SHA256 EXE: f1e776697f65c6e521434eac54fae85bd787b56979722ce946710eab733f30b6
Versiones FileVersion/ProductVersion: 0.6.8. Ejecutable sin firma Authenticode.

## Pendiente de producción

Los seis diseños están recortados y preparados localmente, pero no están publicados en la biblioteca compartida. Falta acceso autenticado al Supabase y Render existentes. No se ha desplegado el backend ni cargado objetos. El paquete podrá mostrar 56 diseños después de activar ese catálogo; mientras tanto admite el catálogo existente de 50.

## Publicación verificada — 17 de octubre de 2026

La fase pendiente anterior está completada. Los seis objetos se subieron al bucket privado existente y se verificaron mediante descarga y SHA256. Render desplegó el commit c275c7f31ab60fbfb2b694fc36a76358279a33b4: despliegue dep-db9mcmvavr4c73ek61d0, estado Live, 42,7 segundos. No se cambiaron planes, secretos ni la rama configurada.

Prueba real con una sesión vigente de la app: catálogo antiguo 50, catálogo ampliado 56, seis descargas verificadas por tamaño y SHA256. Petición sin sesión: 401. /health correcto. Sin consultar copias bancarias ni datos de tarjetas. Wallet Studio 0.6.8 permite actualizar la biblioteca y ver los seis nuevos diseños; versiones anteriores conservan sus 50.
