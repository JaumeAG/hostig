# ADR 0008 — Diagnóstico con IA opcional

- Estado: Aceptado
- Fecha: 2026-10-20

## Contexto

Leer un error de Apache o de InnoDB lleva tiempo. Una explicación en lenguaje normal ayuda, y no puede ser un requisito: la prioridad es que el stack arranque y pare sin red de un proveedor de modelos y sin una clave.

El diagnóstico ve logs que pueden contener rutas locales, nombres de bases y consultas. Ese material no debe salir del equipo salvo que la persona haya elegido un proveedor remoto y haya confirmado el envío.

## Decisión

`diagnose` es un módulo que el supervisor carga siempre y que, sin proveedor, responde «la IA no está configurada» junto con el extracto de log que ya habría enviado. Arrancar, parar, proyectos, puertos y backups no lo llaman.

Proveedores, uno activo a la vez, guardado en la configuración local:

1. **Ollama en la máquina**, por defecto `http://127.0.0.1:11434`. No hace falta clave. El módulo comprueba ese endpoint y lista los modelos que Ollama reporte. El tráfico no sale del equipo si Ollama está en loopback.
2. **API compatible con OpenAI** usando una clave que pone el usuario. La clave se guarda en el llavero del sistema (Credential Manager, Keychain, libsecret). No se escribe en `project.toml`, ni en el repositorio, ni en el log. El endpoint por defecto es el de OpenAI; se puede cambiar la URL base para otro proveedor compatible. No se integran SDK de varios vendedores en la primera versión.

El prompt incluye solo: sistema operativo, versiones de los runtimes en uso, las últimas líneas relevantes del log (con un tope de 32 KiB) y el `cause`/`action` que el supervisor ya emitió. No incluye el contenido de las bases ni el directorio del código fuente más allá de la ruta del log.

Antes del primer envío a un proveedor que no sea loopback, la UI muestra el extracto y pide confirmación. Ollama en loopback no pide esa confirmación cada vez; la configuración inicial sí explica que el texto del log llega al modelo local.

No hay telemetría de diagnósticos hacia BITXMP. No hay modelo embebido en el instalador.

Si el proveedor falla, el módulo devuelve el error de red y el mismo extracto. El resto de la app sigue igual.

## Consecuencias

- El producto se puede usar y probar sin Ollama y sin claves.
- Un modelo local grande choca con la prioridad de eficiencia. Por eso Ollama es optativo y no se arranca desde el supervisor.
- Una URL base arbitraria puede apuntar a un servidor que la persona no quería. La confirmación del extracto es la barrera; la URL se muestra en ese diálogo.
- Rotar o borrar la clave es una operación del módulo de ajustes, no un edit de archivo en el repo.

## Alternativas descartadas

- IA obligatoria en el asistente de errores. Deja el producto inútil sin red o sin clave.
- Varios SDK (Anthropic, Google, OpenAI) en la primera versión. Un cliente HTTP compatible con la API de OpenAI cubre el caso remoto con menos superficie.
- Enviar el log entero del datadir. Puede contener consultas. El tope y el filtro por el incidente actual lo acotan.
