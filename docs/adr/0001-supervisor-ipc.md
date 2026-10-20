# ADR 0001 — Supervisor independiente e IPC local

- Estado: Aceptado
- Fecha: 2026-10-20

## Contexto

Hostig tiene que mantener Apache, PHP y MariaDB vivos aunque la ventana se cierre, y la misma operación debe existir en una CLI. Si el proceso de la UI fuera el padre de los servidores, cerrar la ventana o un fallo del webview tiraría los servicios. XAMPP mezcla panel, scripts `.bat` y el estado real del proceso; el análisis de `C:\xampp_8_2` muestra PID files atrasados y una parada que no pasa por el servidor de base de datos ([xampp-analisis.md](../research/xampp-analisis.md)).

Tauri v2 ya tiene IPC entre el webview y su proceso Rust. Ese canal no sirve para la CLI ni sobrevive al cierre de la ventana.

Hace falta un transporte que exista en Windows, macOS y Linux, que no pida un puerto TCP y que un proceso de otro usuario no pueda usar.

## Decisión

Hay tres procesos de producto:

1. `hostig-supervisor`, demonio de usuario. Es el único que arranca y para servidores.
2. La app Tauri (React). Es un cliente.
3. `hostig`, la CLI. Es un cliente.

Si el supervisor no está en marcha, la UI o la CLI lo arrancan y se conectan. Un segundo arranque ve el lock y solo se conecta. Cerrar la UI no para el supervisor ni los servidores. Parar el supervisor es un comando explícito, y antes aplica el apagado ordenado del [ADR 0003](0003-supervision-de-procesos.md).

### Transporte

Un socket de flujo, solo local, con la misma semántica en las tres plataformas:

| Sistema | Mecanismo | Dirección |
|---|---|---|
| Linux | Socket de dominio Unix | `$XDG_RUNTIME_DIR/hostig/supervisor.sock`, o `/run/user/$UID/hostig/supervisor.sock` si `XDG_RUNTIME_DIR` no existe |
| macOS | Socket de dominio Unix | `~/Library/Application Support/Hostig/run/supervisor.sock` |
| Windows | Named pipe | `\\.\pipe\hostig\supervisor` |

El directorio del socket queda en modo `0700` y el socket en `0600`. En Windows la DACL del pipe concede acceso al usuario actual y a SYSTEM, y a nadie más.

No se usa TCP. Un puerto de control compite con el gestor de puertos del producto, puede provocar un aviso del firewall de Windows y es fácil dejarlo escuchando en una interfaz que no es loopback.

### Protocolo

JSON-RPC 2.0 sobre el socket. Cada mensaje lleva delante su longitud en un entero de 32 bits, little-endian, con un máximo de 1 MiB.

La primera petición de una conexión es `auth`, con el testigo leído de `{datos}/auth.token` (archivo `0600`, creado al primer arranque del supervisor). Además se comprueba la identidad del peer:

- Linux: `SO_PEERCRED`
- macOS: `getpeereid`
- Windows: el SID del cliente del named pipe

Si el usuario del peer no es el dueño del supervisor, la conexión se cierra. El testigo cubre el caso en el que el sistema entrega el socket a otro proceso del mismo usuario que no debería automatizarse sin leer ese archivo; no sustituye al chequeo de peer.

El servidor puede enviar notificaciones JSON-RPC (cambio de estado, progreso de descarga, línea de log) por la misma conexión. La CLI de un solo comando usa petición y respuesta y cierra.

### Qué cruza el IPC

Comandos con nombre (`start`, `stop`, `status`, `ports.check`, `projects.apply`, `runtimes.install`). La UI no envía PID para matar, ni un `my.ini` editado, ni una ruta de sistema. El supervisor traduce el comando a llamadas de sus módulos.

Los errores que cruzan el IPC llevan `cause` y `action`, el mismo criterio que el log estructurado.

## Consecuencias

- El supervisor se puede probar sin abrir la ventana: la CLI y un test hablan el mismo protocolo.
- Hay que empaquetar dos binarios nuestros (supervisor y CLI) además de la app. En el MVP pueden vivir en el mismo instalador; siguen siendo procesos distintos.
- Un fallo del protocolo o del lock deja servidores huérfanos. El arranque siguiente debe adoptar los procesos cuya línea de comandos apunta a esta carpeta de datos, o reportarlos como ajenos si no puede probarlo.
- Tauri IPC queda solo entre React y el proceso de la ventana. Ese proceso es un cliente del supervisor.

## Alternativas descartadas

- El proceso Tauri como padre de Apache y MariaDB. Cerrar la UI mata el stack.
- HTTP en `127.0.0.1`. Mismo código en los tres sistemas, y también avisos de firewall, un puerto más que cuidar y un fallo de bind en `0.0.0.0` que expondría el control.
- gRPC. El plano de control no necesita el peso del codegen.
- stdio exclusivo. Obliga a que cada cliente sea el padre del supervisor, y entonces la UI y la CLI no pueden compartir el demonio.
