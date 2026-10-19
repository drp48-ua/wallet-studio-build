# Actualizaciones para compradores

Las actualizaciones están incluidas indefinidamente, sin suscripción. La aplicación muestra su versión, pero no descarga, instala ni sustituye ejecutables. No existe todavía un servidor de licencias ni un canal automático autenticado. Solicita el acceso mediante la entrega de tu compra o javicarmelosomanager@gmail.com. No hay enlace público de descarga dentro de la aplicación.

## Actualización manual comprobada antes de instalar

El vendedor debe entregar mediante el canal de compra el ZIP, SHA-256, versión y huella del certificado del editor. Una suma recibida junto a un archivo desde un canal comprometido no acredita autenticidad. Verifica la identidad del editor por un canal fiable; nunca aceptes una huella arbitraria sugerida por un archivo desconocido.

1. Obtén la nueva versión desde tu compra. No uses enlaces públicos de GitHub como mecanismo de autorización.
2. Cierra Wallet Studio y espera a que no haya operaciones pendientes. Conserva la carpeta anterior completa, especialmente designs, y todo el perfil local con copias originales y diarios. No compartas esas copias.
3. Usa el comprobador de una entrega anterior de confianza, o una copia facilitada por soporte por el canal autorizado, **antes** de extraer/reemplazar el programa. Ejemplo de parámetros (sustituye los valores; no son hashes reales):

```powershell
./VERIFICAR-DESCARGA.ps1 -ZipPath ./Wallet-Studio-0.5.4-Windows.zip -ExpectedVersion 0.5.4 -CurrentVersion 0.5.3 -ExpectedSha256 '<SHA256 comunicado por el vendedor>' -ExpectedSignerThumbprint '<huella del editor verificada>'
```

El comprobador exige versión posterior, SHA-256 exacto del ZIP, plataforma x64, paquete definitivo, versión interna del EXE, firma Authenticode válida y huella esperada. No instala ni ejecuta el EXE. La primera instalación necesita verificar manualmente SHA-256 y Propiedades → Firmas digitales con los datos fiables del vendedor; el comprobador solo sirve para actualizaciones.

4. Si alguna comprobación falla, no sustituyas archivos. Si pasa, extrae a una carpeta nueva y conserva tus diseños. Abre únicamente la versión nueva cuando hayas cerrado la anterior. No borres el perfil local. No vuelvas a un cliente incompatible con diarios de esquema 2.

Cambiar de certificado requiere comunicar y verificar la nueva huella antes de actualizar. No basta con editar un manifiesto.

## Preparación del vendedor

Compila, prueba, firma con certificado de editor y ejecuta `./scripts/package-windows.ps1`. No incluye claves privadas; mantenlas fuera del repositorio y del binario. El comando exige avisos de terceros completos y firma válida. `-Draft` produce exclusivamente un borrador marcado NO-VENDER para inspección. Las sumas internas verifican cada archivo; el empaquetador reabre el ZIP y comprueba contenidos y hashes.

La distribución automática futura requiere acceso de comprador, manifiesto firmado (versión, plataforma, tamaño y hash), clave pública fijada en el cliente, protección contra retroceso y sustitución atómica con la app cerrada. El hash no sustituye la firma. No se ha implementado ese actualizador. La propuesta de activaciones y descargas privadas está en PREPARACION-COMERCIAL.md.
