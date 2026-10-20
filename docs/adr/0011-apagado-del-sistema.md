# ADR 0011 — Apagado del sistema, cierre de sesión y suspensión

- Estado: Aceptado
- Fecha: 2026-10-20

## Contexto

El supervisor es un proceso de usuario, sin ventana y sin consola, y no es un servicio de Windows. Cerrar la UI no lo para ([ADR 0001](0001-supervisor-ipc.md)). El sistema sí puede apagarse, cerrar la sesión o suspenderse con los servidores todavía en marcha.

En Windows un proceso sin consola no recibe `CTRL_SHUTDOWN_EVENT`, y un proceso sin ventana no recibe `WM_QUERYENDSESSION`. Si nadie escucha, el sistema acaba en `TerminateProcess`. MariaDB se queda a medias y el arranque siguiente entra en la recuperación que el análisis de XAMPP ya mostró repetida.

Hay un segundo riesgo, distinto del apagado: agrupar procesos para poder señalarlos. En Windows, un Job Object con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` mata a todos sus miembros cuando se cierra el último handle del job. Si MariaDB está en ese job y el supervisor muere, la base muere con él, sin `mariadb-admin shutdown`.

El volcado del [ADR 0007](0007-backups.md) no cabe en este camino. Un apagado del sistema da unos segundos, no el tiempo de un dump.

## Decisión

El supervisor distingue tres avisos: apagado, cierre de sesión y suspensión. Los dos primeros comparten una parada ordenada rápida y sin volcado. La suspensión no para la base si no da tiempo.

### Cómo llega el aviso

**Windows.** El supervisor crea una ventana solo de mensajes (`HWND_MESSAGE`) y un bucle de mensajes en un hilo dedicado. Esa ventana es el mecanismo explícito que un proceso sin ventana no tiene de otra forma.

- Apagado: `WM_QUERYENDSESSION` y `WM_ENDSESSION`.
- Cierre de sesión: el mismo par, con `ENDSESSION_LOGOFF`, y además `WTSRegisterSessionNotification` (`WTS_SESSION_LOGOFF`).
- Suspensión y reanudación: `RegisterSuspendResumeNotification` o, en su defecto, `WM_POWERBROADCAST` con `PBT_APMSUSPEND` y `PBT_APMRESUMEAUTOMATIC`.

Mientras dura la parada rápida se llama a `ShutdownBlockReasonCreate`. Al terminar, o al vencer el plazo, `ShutdownBlockReasonDestroy`, para no retener el apagado. No se registra un servicio de Windows para recibir `SERVICE_CONTROL_PRESHUTDOWN`: el MVP sigue siendo un proceso de usuario.

**Linux.** `SIGTERM` es la parada rápida. Además, el supervisor toma un inhibidor de retardo de logind (`PrepareForShutdown` y `PrepareForSleep` por D-Bus) para disponer de esos segundos. No se usa `PR_SET_PDEATHSIG` sobre MariaDB: esa marca mataría a la base cuando muriera el supervisor, que es el fallo que este ADR evita.

**macOS.** Un `CFRunLoop` registra `IORegisterForSystemPower` y atiende `kIOMessageSystemWillPowerOff` y `kIOMessageSystemWillSleep`. Al acabar la parada rápida, o al vencer el plazo, llama a `IOAllowPowerChange`. El cierre de sesión llega como `SIGTERM` y luego `SIGKILL` si el proceso no escucha; el supervisor también se registra a las notificaciones de apagado de la sesión (`NSWorkspace`) para entrar en el mismo camino antes de esa señal.

### Parada rápida, sin volcado

En apagado y en cierre de sesión no se ejecuta `mariadb-dump`. No se crea una copia a medias ni se marca ninguna copia como correcta.

Orden, el mismo del [ADR 0003](0003-supervision-de-procesos.md), con plazos distintos: Apache deja de aceptar, salen los pools PHP, y MariaDB recibe `mariadb-admin shutdown`. El objetivo es unos 5 segundos. El tope duro es 20 segundos para todo el camino. No se usa la espera de 30 segundos del `stop` pedido por la persona.

Si el plazo vence, se registra el error con la causa y la acción, se suelta el bloqueo del apagado y se deja que el sistema siga. No hay `taskkill` por nombre de imagen ni `TerminateProcess` sobre MariaDB en este camino.

### Si el sistema mata el proceso

`TerminateProcess` o `SIGKILL` no dejan ejecutar más limpieza. El arranque siguiente aplica el [ADR 0003](0003-supervision-de-procesos.md): si el PID vive y la línea de comandos es nuestra, se adopta; si no vive, se marca caído, no se borra el datadir, y MariaDB pasa por recuperación y por la regla del LSN. Un volcado que no llegó a escribirse no cuenta como copia.

### Suspensión

No se inicia un volcado. No se inicia una parada larga. Si el plazo no da para un `shutdown` completo, MariaDB se queda en marcha. En la reanudación el supervisor repite el health check. Solo vuelve a arrancar lo que él mismo hubiera parado para esa suspensión.

### Job Objects y la muerte del supervisor

Ningún Job Object que contenga a MariaDB lleva `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.

MariaDB se crea con `CREATE_BREAKAWAY_FROM_JOB` cuando el job padre permite salir. Si el sistema niega el breakaway, el supervisor no arranca MariaDB dentro de ese job: el error dice la causa y la acción (arrancar el supervisor fuera de ese job). La muerte del supervisor tiene que dejar la base viva para que el arranque siguiente pueda adoptarla.

Apache y PHP pueden ir en un job propio si el test de cierre de Apache Lounge del [ADR 0003](0003-supervision-de-procesos.md) lo necesita para un ctrl-break. Ese job no incluye a MariaDB y tampoco usa kill-on-close.

## Consecuencias

- Hace falta un hilo con bucle de mensajes en Windows aunque el supervisor no tenga UI.
- Un apagado del sistema puede cortar peticiones. Es preferible a un kill de MariaDB, y no sustituye al `stop` normal, que sí puede volcar.
- Si el breakaway está denegado, la base no arranca. El mensaje tiene que decirlo antes de dejar un datadir a medias.
- La suspensión que deja MariaDB en marcha puede coincidir con un portátil que duerme a mitad de una escritura. InnoDB ya hace esa recuperación; el supervisor no la adelanta con un kill.

## Alternativas descartadas

- Instalar el supervisor como servicio de Windows para recibir el aviso de apagado. Pide administrador y mezcla el producto con el sistema. El MVP no lo hace.
- Volcar la base al recibir el aviso. No cabe en el plazo, y un volcado cortado por `SIGKILL` parecería una copia.
- `PR_SET_PDEATHSIG`, o un job con kill-on-close, para «no dejar huérfanos». Deja a MariaDB muerto sin shutdown en cuanto el supervisor desaparece.
- Tratar la suspensión como un apagado y parar siempre. Un portátil que duerme no debería exigir un dump ni un kill.
