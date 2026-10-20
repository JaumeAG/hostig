# ADR 0010 — Distribución, firma y actualizaciones

- Estado: Aceptado como estrategia. Las cuentas de firma son un requisito tuyo, no un detalle de implementación.
- Fecha: 2026-10-20

## Contexto

El producto tiene que existir en Windows, macOS y Linux. Tauri v2 ya define el empaquetado, el plugin de actualizaciones y la firma de esos paquetes. La documentación actual del updater exige una firma; no se puede desactivar. Las claves se generan con `tauri signer generate`. La pública va en la configuración de la app. La privada firma los artefactos en CI y no se recupera si se pierde: los clientes ya instalados dejarían de poder actualizar.

Los runtimes no viajan en ese paquete ([ADR 0002](0002-gestor-de-runtimes.md), [ADR 0009](0009-licencia.md)).

Firma de sistema operativo y firma del updater son dos cosas distintas. La del updater demuestra que el paquete lo produjo esta clave. La del sistema (Authenticode, Developer ID de Apple, y en Linux poco más que el propio archivo) es la que evita el aviso de «publicador desconocido».

## Decisión

### Pipeline

GitHub Actions con `tauri-action`, en tags `v*`:

| Runner | Artefacto de app | Artefacto del updater |
|---|---|---|
| `windows-latest` | Instalador NSIS (`.exe`) | El `.sig` que Tauri genera cuando `createUpdaterArtifacts` está activo |
| `macos-latest` | `.app` en `.dmg`, por arquitectura | `app.tar.gz` y su `.sig` |
| `ubuntu-22.04` o el que fije Tauri v2 en ese momento | AppImage | El `.sig` del AppImage |

El MVP funcional es Windows x64. Linux es la fase 1b y macOS la fase 1c, cada una con la misma definición de hecho. Aun así, el CI compila la aplicación y ejecuta sus tests en Windows, Linux y macOS desde el primer workflow. macOS aarch64 entra con la fase 1c. Linux aarch64 y Windows ARM quedan fuera de esas fases.

El workflow publica una GitHub Release con los instaladores y un `latest.json` cuyas URLs apuntan a los assets de esa release. El endpoint del plugin updater es esa release. Los secretos del repo son `TAURI_SIGNING_PRIVATE_KEY` y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. La clave privada no se imprime en logs. Una copia fuera de GitHub (gestor de contraseñas) es obligatoria el día que se genere: el ADR no puede guardarla por ti.

Los runtimes compilados por nosotros salen de un workflow distinto, `runtimes.yml`, en la fase que corresponde (Linux en la 1b, macOS en la 1c). No firma con la clave del updater de la app. Su integridad es el SHA-256 del catálogo.

### Firma de sistema

Primero Windows. macOS se firma cuando llegue su fase, no antes.

- **Windows.** Se evalúa Azure Trusted Signing, el servicio que Microsoft documenta ahora también como Azure Artifact Signing (<https://learn.microsoft.com/en-us/azure/trusted-signing/quickstart>, <https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options>). Firma en la nube, sin token de hardware, desde los runners de Windows de GitHub (acción oficial, con OIDC). No da reputación instantánea en SmartScreen: el aviso puede seguir hasta que el certificado acumule reputación. La validación de identidad de confianza pública admite organizaciones en una lista de países (entre ellos Estados Unidos, Canadá, la Unión Europea y el Reino Unido) y, para una persona física, solo Estados Unidos o Canadá. Quien publique como particular fuera de esos dos países no puede usar esa confianza pública; el respaldo es un certificado Authenticode OV tradicional. Hasta que exista una de las dos firmas, SmartScreen avisará. No se desactiva ningún control del sistema para evitarlo.
- **macOS.** Cuando llegue la fase 1c y se vaya a distribuir al público: cuenta de Apple Developer, certificado Developer ID Application y notarización. Sin eso, Gatekeeper trata el binario descargado como dañado o no identificado. Los builds de macOS anteriores a esa fase sirven para probar en local y no se presentan como release pública.
- **Linux.** AppImage más la firma del updater. No hay un almacén de confianza equivalente que estas fases vayan a integrar.

El helper privilegiado ([ADR 0006](0006-privilegios.md)) tiene que ir firmado con la misma identidad que la app. En Windows, la misma firma Authenticode que se evalúa arriba. En macOS, la misma identidad de Developer ID, cuando esa fase firme.

### Actualizaciones

- Se actualiza la app, la CLI y el supervisor, que viajan juntos en el instalador.
- No se autoactualiza un runtime. Cambiar Apache, PHP o MariaDB es una acción del gestor de runtimes, con backup previo si hay una base ([ADR 0007](0007-backups.md)), y la persona la confirma.
- El updater comprueba la firma antes de sustituir archivos. Si la firma falla, no instala y lo registra.
- Una actualización no para MariaDB a mitad de una petición sin pasar por el apagado ordenado. El flujo es: la app descarga el paquete, pide al supervisor un `stop` (con backup), aplica el update, y el siguiente arranque lo hace el usuario o la app al reabrir. No hay un servicio que mate los servidores para poder reemplazar el `.exe` del supervisor.

### Eficiencia del CI

Cache de Rust y de npm. Los builds de los tres sistemas van en paralelo. El workflow de runtimes no se dispara en cada commit de la UI: solo cuando cambia el catálogo o el script de build del prefijo.

## Consecuencias

- Hace falta una organización de GitHub. La firma pública de Windows se intenta primero, con la salvedad de identidad de Azure Trusted Signing. La cuenta de pago de Apple espera a la fase de macOS.
- Perder la clave del updater corta la cadena de actualizaciones. Hay que rotarla generando una release que los clientes antiguos no recibirán solos; sería una reinstalación.
- AppImage en Linux falla en algunos entornos con FUSE. Se documenta la alternativa `APPIMAGE_EXTRACT_AND_RUN` y, si duele de verdad, un `.deb` queda para después del MVP. No se promete paquete de distro en la primera versión.
- El supervisor y la app comparten versión. No se soporta una app nueva hablando con un supervisor viejo más de una versión menor: el handshake del IPC incluye la versión del protocolo y rechaza incompatibles con un mensaje que pide actualizar.

## Alternativas descartadas

- Auto-update por reemplazar archivos sin firma. Tauri no lo permite, y no deberíamos quererlo.
- Incluir los runtimes en el instalador para que el primer arranque no descargue. Contradice el ADR 0002 y engorda cada update de la UI con cientos de MB.
- Microsoft Store o Mac App Store en el MVP. Cambian privilegios, sandbox y el helper. Se pueden estudiar cuando el modo sin privilegios ya esté sólido.
