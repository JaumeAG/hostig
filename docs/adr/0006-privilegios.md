# ADR 0006 — Privilegios

- Estado: Aceptado
- Fecha: 2026-10-20

## Contexto

Puertos por debajo de 1024 y el archivo hosts piden un proceso elevado en los tres sistemas. Confiar una CA local también. El resto del producto (descargar runtimes, supervisar procesos, generar config, hablar por el socket del usuario) no lo pide.

Un supervisor entero elevado amplía el daño de un fallo. XAMPP ya enseñaba el otro extremo: sin admin, el panel avisaba de que los servicios se rompen, y con scripts elevados a medias la parada ni siquiera apuntaba al ejecutable correcto.

La regla del proyecto prohíbe tocar PATH, servicios de Windows, hosts o puertos del sistema sin una confirmación explícita en ese momento.

## Decisión

El supervisor y la UI corren como el usuario. Un helper aparte, `hostig-helper`, es el único binario que se eleva, y solo sabe hacer cuatro operaciones:

1. Insertar o quitar un bloque marcado dentro del archivo hosts (`# hostig-begin` … `# hostig-end`). Fuera del bloque no se reescribe nada.
2. Instalar o retirar la CA local del almacén de confianza del sistema (máquina en Windows, System keychain en macOS, el almacén que use la distro en Linux).
3. Reenviar 80 y 443 al puerto alto en el que Apache ya escucha. Es el único mecanismo, y es el mismo en Windows, macOS y Linux. El helper instala un reenvío reversible: portproxy en Windows, `pf` en macOS, nftables en Linux. Apache sigue siendo un proceso del usuario, atado al puerto alto. La UI tiene que mostrar que el proceso que escucha en 80 no es Apache. La operación 4 deshace el reenvío junto con el bloque de hosts y la CA.
4. Deshacer 1, 2 y 3 por completo (`hostig privileges reset`).

El helper no acepta un shell, ni una ruta arbitraria, ni un `taskkill`. Cada invocación recibe un subcomando y argumentos acotados (hostname ya validado como label DNS, puerto de destino que ya pasó por el módulo de puertos). La UI pide confirmación en el momento, con el texto de lo que va a cambiar.

Elevación por plataforma:

| Sistema | Cómo se eleva el helper |
|---|---|
| Windows | Ejecutable que pide UAC en ese subcomando, no un servicio permanente |
| macOS | Helper privilegiado registrado con SMAppService, prompt del sistema |
| Linux | Política polkit que autoriza solo esos subcomandos |

### Modo sin privilegios

Siempre disponible, y es el modo del MVP:

- HTTP en un puerto alto (8080 por defecto).
- MariaDB en 3306 o el siguiente libre del rango. 3306 no es un puerto privilegiado.
- Dominio `{slug}.localhost` en vez de `{slug}.test`.
- HTTPS apagado hasta que la CA esté confiada, para no entrenar al usuario a saltarse avisos del navegador.

Pasar al modo con helper es optativo y reversible.

## Consecuencias

- El MVP no incluye el helper. Arrancar y parar el stack no depende de él.
- El reenvío 80→8080 es un comportamiento distinto de «Apache escucha en 80». Hay que mostrarlo en la UI para que la persona sepa dónde está el certificado y el log.
- Una política polkit o un helper de macOS es código con superficie de seguridad. Se revisa aparte y sus operaciones tienen tests de «rechaza argumentos que no están en la lista».
- Desinstalar Hostig sin `privileges reset` dejaría líneas en hosts y una CA confiada. El desinstalador llama a reset si el helper sigue presente, y la documentación dice cómo quitar el bloque a mano si no.

## Alternativas descartadas

- Ejecutar Apache como administrador o como root. El worker HTTP no necesita ese poder si el reenvío vive en el helper.
- Escribir el archivo hosts desde el supervisor cuando «parece» que hay permisos. A veces los hay por un accidente de ACL y el producto se acostumbra.
- Obligar a admin para cualquier uso. Contradice el modo sin privilegios y el MVP.
