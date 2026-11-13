import { html, render, useState, useEffect, useRef, useMemo } from './preact.js';

// ───────────── i18n ─────────────
// English is the source language; ES maps each English string to its Spanish translation.
// A `word|` prefix gives context when one English string needs two Spanish ones; it's stripped in English.
const LANG = ['en', 'es'].includes(localStorage.lang) ? localStorage.lang
  : [...(navigator.languages || [navigator.language])].some((l) => /^es/i.test(l)) ? 'es' : 'en';
document.documentElement.lang = LANG;
const LOCALE = LANG === 'es' ? 'es-MX' : /^en/i.test(navigator.language) ? navigator.language : 'en-US';
const ES = {
  // dates, network, uploads
  'Overdue': 'Vencido', 'Due today': 'Vence hoy', 'Due in {n} day': 'Vence en {n} día', 'Due in {n} days': 'Vence en {n} días',
  'The upload was cut off before it finished. Please try again.': 'La subida se cortó antes de terminar. Inténtalo de nuevo.',
  'No connection to Papiro': 'Sin conexión con Papiro',
  '{file} was already in the archive': '{file} ya estaba en el archivo',
  'Your user is read-only': 'Tu usuario es de solo lectura',
  'document': 'documento',
  'Uploading {name}…': 'Subiendo {name}…',
  'Uploading {name} to “{folder}”…': 'Subiendo {name} a «{folder}»…',
  'Uploading {n} files…': 'Subiendo {n} archivos…',
  'Uploading {n} files to “{folder}”…': 'Subiendo {n} archivos a «{folder}»…',
  'The browser couldn’t read “{name}”. Try the Upload button or copy the file to another folder.': 'El navegador no pudo leer «{name}». Prueba con el botón Subir o copia el archivo a otra carpeta.',
  // folders
  'New folder': 'Nueva carpeta', 'New folder in “{name}”': 'Nueva carpeta en «{name}»', 'Folder name': 'Nombre de la carpeta', 'Create': 'Crear',
  'Rename': 'Cambiar nombre', 'Save': 'Guardar',
  'Delete the folder “{name}”?': '¿Borrar la carpeta «{name}»?',
  'Its documents aren’t deleted: they move to “{name}”.': 'Sus documentos no se borran: pasan a «{name}».',
  'Its documents aren’t deleted: they’re left without a folder.': 'Sus documentos no se borran: se quedan sin carpeta.',
  'Its subfolders are deleted too.': 'Sus subcarpetas también se borran.',
  'Delete folder': 'Borrar carpeta', 'Folder deleted': 'Carpeta borrada', 'New subfolder': 'Nueva subcarpeta',
  'Moved to “{name}”': 'Movido a «{name}»', 'Removed from its folder': 'Sacado de su carpeta',
  'Folder options': 'Opciones de la carpeta', 'Collapse': 'Contraer', 'Expand': 'Desplegar',
  // pickers, dialogs
  'Remove': 'Quitar', 'Search or create…': 'Buscar o crear…', 'Search…': 'Buscar…', 'Create “{name}”': 'Crear «{name}»', 'No results': 'Sin resultados',
  'Search or create a folder…': 'Buscar o crear carpeta…', 'No folder': 'Sin carpeta', 'Create folder “{name}”': 'Crear carpeta «{name}»',
  'No folders yet: type a name to create the first one': 'Aún no hay carpetas: escribe un nombre para crear la primera',
  'Cancel': 'Cancelar', 'OK': 'Aceptar',
  // sign-in
  'Create your admin account': 'Crea tu cuenta de administrador', 'Sign in': 'Inicia sesión',
  'This is the first time this installation has been opened. This account will have full control.': 'Es la primera vez que se abre esta instalación. Esta cuenta tendrá control total.',
  'Username': 'Usuario', 'Password': 'Contraseña', 'Create account': 'Crear cuenta', 'Log in': 'Entrar',
  // sidebar, tab bar
  'Close': 'Cerrar', 'Upload documents': 'Subir documentos', 'Upload documents (U)': 'Subir documentos (U)',
  'Inbox': 'Bandeja de entrada', 'short|Inbox': 'Bandeja', 'All documents': 'Todos los documentos', 'Due dates': 'Vencimientos', 'Trash': 'Papelera',
  'Folders': 'Carpetas', 'Create your first folder': 'Crea tu primera carpeta', 'No folders yet': 'Aún no hay carpetas',
  'Folder': 'Carpeta', 'Tag': 'Etiqueta', 'Tags': 'Etiquetas', 'Correspondent': 'Corresponsal', 'Correspondents': 'Corresponsales', 'Type': 'Tipo', 'Types': 'Tipos',
  'Processing {n}…': 'Procesando {n}…', 'Activity': 'Actividad', 'Failed jobs': 'Trabajos fallidos', 'Install Papiro': 'Instalar Papiro',
  'Settings': 'Ajustes', 'Light mode': 'Modo claro', 'Dark mode': 'Modo oscuro', 'Archive': 'Archivo', 'Add documents': 'Añadir documentos', 'More': 'Más',
  'Add to Papiro': 'Añadir a Papiro', 'Add to “{name}”': 'Añadir a «{name}»',
  'Take a photo': 'Tomar una foto', 'Photograph a paper document; Papiro extracts the text.': 'Fotografía un documento en papel; Papiro extrae el texto.',
  'Choose files': 'Elegir archivos', 'PDF, photos, Word, Excel, PowerPoint or text.': 'PDF, fotos, Word, Excel, PowerPoint o texto.',
  'With Papiro installed on your phone, you can also use “Share” from any other app.': 'Con Papiro instalada en el teléfono también puedes usar «Compartir» desde cualquier otra app.',
  // activity
  'Queued': 'En cola', 'Extracting text…': 'Extrayendo texto…', 'Done': 'Listo', 'Already in the archive': 'Ya estaba en el archivo', 'Failed': 'Falló',
  'Nothing has been processed yet. Upload a file to get started.': 'Aún no se ha procesado nada. Sube un archivo para empezar.',
  'open': 'abrir', 'Retrying…': 'Reintentando…', 'Retry': 'Reintentar', 'Dismiss': 'Descartar', 'The file was kept': 'El archivo se conservó',
  // list
  'Has suggestions': 'Tiene sugerencias', '{n} page': '{n} página', '{n} pages': '{n} páginas',
  'Relevance': 'Relevancia', 'Due soonest': 'Vence antes', 'Newest': 'Más recientes', 'Oldest': 'Más antiguos', 'Recently added': 'Recién añadidos', 'Title A–Z': 'Título A–Z',
  'Nothing matches your search': 'Nada coincide con tu búsqueda', 'Try other words or remove a filter.': 'Prueba con otras palabras o quita algún filtro.',
  'This folder is empty': 'Esta carpeta está vacía',
  'Anything you upload while here is saved in this folder. You can also move documents from their details.': 'Lo que subas estando aquí se guarda en esta carpeta. También puedes mover documentos desde su detalle.',
  'There are no documents here yet.': 'Todavía no hay documentos aquí.',
  'Inbox zero': 'Bandeja al día', 'New items you haven’t reviewed yet will show up here.': 'Aquí aparecerá lo nuevo que aún no has revisado.',
  'Nothing due soon': 'Nada vence pronto', 'Documents that are overdue or due in the next 60 days show up here.': 'Aquí aparecen los documentos vencidos o que vencen en los próximos 60 días.',
  'The trash is empty': 'La papelera está vacía', 'What you delete stays here until you restore it or delete it for good.': 'Lo que borres se queda aquí hasta que lo restaures o lo borres del todo.',
  'Your archive is empty': 'Tu archivo está vacío', 'Tap + to photograph a document or choose files.': 'Toca + para fotografiar un documento o elegir archivos.',
  'Drag your PDFs, photos or Office documents here to get started.': 'Arrastra aquí tus PDF, fotos o documentos de Office para empezar.',
  'Search in {name}…': 'Buscar en {name}…', 'Search documents…': 'Buscar documentos…', '{user}: settings': '{user}: ajustes',
  'From': 'Desde', 'To': 'Hasta', 'Clear': 'Limpiar',
  '{n} document': '{n} documento', '{n} documents': '{n} documentos',
  '{n} document · total {amount}': '{n} documento · suman {amount}', '{n} documents · total {amount}': '{n} documentos · suman {amount}',
  'Sort': 'Ordenar', 'Change view (V)': 'Cambiar vista (V)',
  // detail
  'Couldn’t show page {n}': 'No se pudo mostrar la página {n}',
  'The extracted text is still available in the Text tab. If the problem continues, an admin can check it in Settings → Health.': 'El texto extraído sigue disponible en la pestaña Texto. Si el problema continúa, un administrador puede revisarlo en Ajustes → Salud.',
  'Page {n}': 'Página {n}', 'That document no longer exists': 'Ese documento ya no existe', 'Back': 'Volver', 'Back (Esc)': 'Volver (Esc)',
  'Classified with AI': 'Clasificado con IA', 'Delete permanently?': '¿Borrar definitivamente?',
  '“{title}” and its files will be removed from the server. This can’t be undone.': '«{title}» y sus archivos se eliminarán del servidor. No se puede deshacer.',
  'Delete forever': 'Borrar para siempre', 'Permanently deleted': 'Borrado definitivamente', 'Moved to trash': 'Enviado a la papelera',
  'What was clear has been applied; no suggestions left': 'Se aplicó lo que estaba claro; no quedan sugerencias',
  'No suggestions: there are no similar filed documents yet': 'Sin sugerencias: aún no hay documentos parecidos ya archivados',
  'Open the PDF in a new tab': 'Abrir el PDF en otra pestaña', 'Download the original ({size})': 'Descargar el original ({size})',
  'Download the PDF with text (OCR)': 'Descargar el PDF con texto (OCR)',
  'Pages': 'Páginas', 'Text': 'Texto', 'Details': 'Detalles', 'Open or download': 'Abrir o descargar', 'Download': 'Descargar',
  'This document has no extracted text.': 'Este documento no tiene texto extraído.', 'Title (tap to edit)': 'Título (toca para editar)',
  'In the trash': 'En la papelera', 'Restored': 'Restaurado', 'Restore': 'Restaurar', 'Mark as reviewed': 'Marcar como revisado',
  'Suggestions': 'Sugerencias', 'Accept all': 'Aceptar todas', '{label} · {n} match': '{label} · {n} parecido', '{label} · {n} matches': '{label} · {n} parecidos',
  'Accept': 'Aceptar', 'Based on similar documents you already filed. Computed on your server.': 'Por parecido con documentos que ya archivaste. Se calcula en tu servidor.',
  'Date': 'Fecha', 'Unassigned': 'Sin asignar', 'None': 'Ninguna', 'Add…': 'Añadir…', 'Due date': 'Vence', 'Amount': 'Importe', 'Archive no.': 'N.º archivo',
  'Private': 'Privado', 'Only the uploader and admins can see it': 'Solo lo ven quien lo subió y los administradores',
  'Only you and admins': 'Solo tú y los administradores', 'Visible to all users': 'Visible para todos los usuarios',
  'Assigned by similarity only: {list}.': 'Asignado solo por parecido: {list}.', '{label} “{name}”': '{label} «{name}»',
  'Find suggestions': 'Buscar sugerencias', 'Summary': 'Resumen', 'Regenerate': 'Regenerar', 'Classify with AI': 'Clasificar con IA', 'Fields': 'Campos',
  'Notes': 'Notas', 'Delete note': 'Borrar nota', 'Write a note…': 'Escribe una nota…', 'No notes': 'Sin notas', 'Save note': 'Guardar nota',
  'File': 'Archivo', 'Added {date}': 'Añadido el {date}', 'SHA-256 of the original': 'SHA-256 del original',
  'Delete permanently': 'Borrar definitivamente', 'Move to trash': 'Enviar a la papelera',
  // bulk edit
  'Suggestions accepted on {n} document': 'Sugerencias aceptadas en {n} documento', 'Suggestions accepted on {n} documents': 'Sugerencias aceptadas en {n} documentos',
  'Permanently delete {n} document?': '¿Borrar definitivamente {n} documento?', 'Permanently delete {n} documents?': '¿Borrar definitivamente {n} documentos?',
  'They’ll be removed from the server along with their files. This can’t be undone.': 'Se eliminarán del servidor con sus archivos. No se puede deshacer.',
  'Documents permanently deleted': 'Borrados definitivamente',
  '{n} document selected': '{n} seleccionado', '{n} documents selected': '{n} seleccionados',
  'Documents restored': 'Restaurados', 'Accept suggestions': 'Aceptar sugerencias', 'Move': 'Mover', 'Add tag': 'Etiquetar',
  'Tag “{name}” added': 'Etiqueta «{name}» añadida', 'Correspondent: {name}': 'Corresponsal: {name}', 'Type: {name}': 'Tipo: {name}',
  'Documents marked as reviewed': 'Marcados como revisados', 'Reviewed': 'Revisado', 'Documents moved to trash': 'Enviados a la papelera',
  'Clear selection (Esc)': 'Quitar selección (Esc)',
  // command palette
  'Go to the inbox': 'Ir a la bandeja de entrada', 'Go to all documents': 'Ir a todos los documentos', 'Go to the trash': 'Ir a la papelera', 'Go to due dates': 'Ir a vencimientos',
  'Switch to light mode': 'Cambiar a modo claro', 'Switch to dark mode': 'Cambiar a modo oscuro',
  'API tokens and MCP': 'Tokens de API y MCP', 'Archive health': 'Salud del archivo', 'API documentation': 'Documentación de la API',
  'Keyboard shortcuts': 'Atajos de teclado', 'Language': 'Idioma', 'Language: {name}': 'Idioma: {name}', 'Sign out': 'Cerrar sesión',
  'Folder: {name}': 'Carpeta: {name}', 'Tag: {name}': 'Etiqueta: {name}',
  'Search for a document or type an action…': 'Busca un documento o escribe una acción…', 'No matches': 'Nada coincide',
  // settings: labels
  'No rule': 'Sin regla', 'Any word': 'Alguna palabra', 'All words': 'Todas las palabras', 'Exact phrase': 'Frase exacta', 'Regular expression': 'Expresión regular',
  'New name…': 'Nombre nuevo…', 'Add': 'Añadir', 'Name': 'Nombre', 'Assign if the text contains': 'Asignar si el texto contiene', 'Rule': 'Regla',
  'Added to every new document': 'Se asigna a todo documento nuevo', 'e.g. ACME electricity': 'p. ej. CFE electricidad',
  'Delete': 'Borrar', 'Delete “{name}”?': '¿Borrar «{name}»?', 'The documents are kept; they only lose this assignment.': 'Los documentos se conservan; solo pierden esta asignación.',
  'Tags marked as “Inbox” are added automatically to every new document; remove them once you’ve reviewed it.': 'Las etiquetas marcadas como «Bandeja» se ponen solas a cada documento nuevo; quítalas al revisarlo.',
  'Who issues or sends the document: companies, banks, agencies, people.': 'Quién emite o envía el documento: empresas, bancos, dependencias, personas.',
  'Document types': 'Tipos de documento', 'Invoice, contract, receipt, policy… Rules assign the type on ingestion.': 'Factura, contrato, recibo, póliza… Las reglas asignan el tipo al ingerir.',
  // settings: tokens, webhooks
  'ppr_YOUR_TOKEN': 'ppr_TU_TOKEN',
  'A token gives scripts, automations or AI assistants access to your archive. It’s shown only once; the server stores only a hash of it.': 'Un token da acceso a tu archivo desde scripts, automatizaciones o asistentes de IA. Solo se muestra una vez; en el servidor se guarda cifrado con hash.',
  'What is it for? (e.g. n8n, Claude)': '¿Para qué es? (p. ej. n8n, Claude)', 'Read-only': 'Solo lectura',
  'Never expires': 'No caduca', 'Expires in {n} days': 'Caduca en {n} días', 'Expires in 1 year': 'Caduca en 1 año', 'Create token': 'Crear token',
  'Copy your token now; it won’t be shown again:': 'Copia tu token ahora; no se volverá a mostrar:',
  'Access': 'Permiso', 'Expires': 'Caduca', 'Last used': 'Último uso', 'Full': 'Completo', 'Never': 'Nunca', 'Token revoked': 'Token revocado', 'Revoke': 'Revocar',
  'Connect an AI (MCP)': 'Conectar una IA (MCP)',
  'Papiro includes an MCP server so an assistant can search, read, upload and tag documents. With a read-only token, it can only search and read.': 'Papiro incluye un servidor MCP para que un asistente busque, lea, suba y etiquete documentos. Con un token de solo lectura, solo puede buscar y leer.',
  'Or through the REST API:': 'O por la API REST:', 'invoice': 'factura',
  'Webhook created': 'Webhook creado',
  'Papiro sends an HTTP POST when a document is created, changed or deleted. With a secret, each delivery is signed (X-Papiro-Signature header, HMAC-SHA256).': 'Papiro avisa por HTTP POST cuando un documento se crea, cambia o se borra. Con secreto, cada aviso va firmado (cabecera X-Papiro-Signature, HMAC-SHA256).',
  'https://your-server/webhook': 'https://tu-servidor/webhook', 'All events': 'Todos los eventos', 'New documents only': 'Solo documentos nuevos', 'New and changed': 'Nuevos y modificados',
  'Signing secret (optional)': 'Secreto de firma (opcional)', 'Events': 'Eventos', 'Last delivery': 'Última entrega', 'signed': 'firmado', 'all': 'todos',
  'Response: {status}': 'Respuesta: {status}', 'Test': 'Probar',
  // settings: account
  'Admin': 'Administrador', 'everything, including permanent deletion and managing users': 'todo, incluido borrar definitivamente y gestionar usuarios',
  'uploads and edits; can’t delete permanently': 'sube y edita; no borra definitivamente', 'Viewer': 'Lector', 'only searches, reads and downloads': 'solo busca, lee y descarga',
  'Password updated': 'Contraseña actualizada', 'User created': 'Usuario creado', 'New password for {user}': 'Nueva contraseña para {user}',
  'At least 8 characters. Their open sessions will be signed out.': 'Mínimo 8 caracteres. Sus sesiones abiertas se cierran.',
  'Reset': 'Restablecer', 'Password reset': 'Contraseña restablecida', 'Account': 'Cuenta', 'Signed in as {user} ({role}).': 'Sesión iniciada como {user} ({role}).',
  'Current password': 'Contraseña actual', 'New (min. 8)': 'Nueva (mín. 8)', 'Change': 'Cambiar', 'Users': 'Usuarios',
  'A document marked as private can only be seen by its uploader and admins.': 'Un documento marcado como privado solo lo ven quien lo subió y los administradores.',
  'Role updated': 'Rol actualizado', 'Reset password': 'Restablecer contraseña', 'Delete the user {user}?': '¿Borrar al usuario {user}?',
  'Their documents are kept.': 'Sus documentos se conservan.', 'Delete user': 'Borrar usuario',
  // settings: health, audit, API
  'Check finished': 'Verificación terminada', 'Health': 'Salud', 'Integrity': 'Integridad', 'Check now': 'Verificar ahora',
  'Papiro {version} · schema {schema} · OCR {langs}.': 'Papiro {version} · esquema {schema} · OCR {langs}.',
  'Papiro {version} · schema {schema} · OCR {langs} with rotated-page correction.': 'Papiro {version} · esquema {schema} · OCR {langs} con corrección de páginas giradas.',
  'The only document has its file and its SHA-256 fingerprint matches.': 'El documento tiene su archivo y su huella SHA-256 coincide.',
  'All {n} documents have their file and their SHA-256 fingerprints match.': 'Los {n} documentos tienen su archivo y su huella SHA-256 coincide.',
  '{n} document has problems.': '{n} documento(s) con problemas.', '{n} documents have problems.': '{n} documento(s) con problemas.',
  'Last check: {date} ({secs} s) · database: {db} · search index: {index}': 'Última verificación: {date} ({secs} s) · base de datos: {db} · índice de búsqueda: {index}',
  'damaged': 'dañado', 'Not checked yet. It runs by itself every day; you can also start it now.': 'Aún no se ha verificado. Se hace sola cada día; también puedes lanzarla ahora.',
  'Backup': 'Respaldo', 'Automatic every {n} h.': 'Automático cada {n} h.', 'turned off': 'desactivado',
  'Automatic backup is {off}. Set {dir} to another volume or disk, or run {cmd} from your own scheduler.': 'El respaldo automático está {off}. Define {dir} apuntando a otro volumen o disco, o ejecuta {cmd} desde tu propio programador.',
  'Last attempt failed: {error}': 'Último intento fallido: {error}',
  'Last backup: {date} · {n} document · {size} in {dest}': 'Último respaldo: {date} · {n} documento · {size} en {dest}',
  'Last backup: {date} · {n} documents · {size} in {dest}': 'Último respaldo: {date} · {n} documentos · {size} en {dest}',
  '{n} daily copy of the database': '{n} copia diaria de la base', '{n} daily copies of the database': '{n} copias diarias de la base',
  'Ingestion': 'Ingesta',
  '{queued} queued · {failed} failed · {hooks} pending webhook deliveries.': '{queued} en cola · {failed} fallidos · {hooks} avisos de webhook pendientes.',
  '{queued} queued · {failed} failed (their files were kept: review them in Activity) · {hooks} pending webhook deliveries.': '{queued} en cola · {failed} fallidos (su archivo se conservó: revísalos en Actividad) · {hooks} avisos de webhook pendientes.',
  'Documents with problems': 'Documentos con problemas',
  'If you still have the file, upload it again: Papiro recognizes it’s the same one and replaces the damaged copy.': 'Si conservas el archivo, vuelve a subirlo: Papiro reconoce que es el mismo y repone la copia dañada.',
  'Audit log': 'Auditoría', 'Who did what and from where: sign-ins, uploads, edits, deletions and admin changes.': 'Quién hizo qué y desde dónde: inicios de sesión, subidas, ediciones, borrados y cambios de administración.',
  'When': 'Cuándo', 'Who': 'Quién', 'What': 'Qué', 'Target': 'Sobre',
  'other': 'otros', 'OpenAPI 3.1 spec:': 'Especificación OpenAPI 3.1:', 'required': 'obligatorio', 'Body': 'Cuerpo', 'Responses': 'Respuestas', 'Example': 'Ejemplo',
  'Tokens & MCP': 'Tokens y MCP',
  // keyboard help
  'Search': 'Buscar', 'Command palette': 'Paleta de comandos', 'Next document': 'Siguiente documento', 'Previous document': 'Documento anterior',
  'Open preview': 'Abrir vista previa', 'Close / clear selection': 'Cerrar / quitar selección', 'Select': 'Seleccionar', 'Shift + click': 'Shift + clic',
  'Select range': 'Seleccionar rango', 'Select all': 'Seleccionar todo', 'Move to a folder': 'Mover a una carpeta', '#  ·  Del': '#  ·  Supr',
  'List / grid': 'Lista / cuadrícula', 'Light / dark mode': 'Modo claro / oscuro', 'G then I · A · T': 'G luego I · A · T', 'Inbox · All · Trash': 'Bandeja · Todos · Papelera',
  'This help': 'Esta ayuda', 'You can do everything without leaving the keyboard.': 'Todo se puede hacer sin soltar el teclado.', 'Marked as reviewed': 'Marcado como revisado',
  // app shell
  'Your documents are on your server: you need a connection to see them. It retries by itself as soon as it’s back.': 'Tus documentos están en tu servidor: hace falta conexión para verlos. Se reintenta solo en cuanto vuelva.',
  'Offline: you’re seeing what was last loaded': 'Sin conexión: verás lo último que se cargó',
  'Drop to add to Papiro': 'Suelta para añadir a Papiro', 'Drop to add to “{name}”': 'Suelta para añadir a «{name}»',
  'PDF, images, Word, Excel, PowerPoint, text': 'PDF, imágenes, Word, Excel, PowerPoint, texto',
};
/** Translates `s` and fills `{name}` placeholders; if a value is a vnode, returns an array for html``. */
function t(s, vars) {
  s = (LANG === 'es' && ES[s]) || s.replace(/^[a-z]+\|/, '');
  if (!vars) return s;
  const parts = s.split(/\{(\w+)\}/).map((p, i) => (i % 2 ? vars[p] ?? `{${p}}` : p));
  return parts.some((p) => p && typeof p === 'object') ? parts : parts.join('');
}
const tn = (n, one, other, vars) => t(n === 1 ? one : other, { n, ...vars });
const setLang = (l) => { localStorage.lang = l; location.reload(); };
// Server messages arrive in English: [englishPattern, spanish] pairs, `{}` marks a dynamic part (substituted in order).
// Filled from the server's message list.
const SERVER_ES = [["{} points to an internal address, which is not allowed (set PAPIRO_WEBHOOK_ALLOW_PRIVATE=true to allow it)","{} apunta a una dirección interna; no se permite (PAPIRO_WEBHOOK_ALLOW_PRIVATE=true para permitirlo)"],["this job cannot be retried (it did not fail, or its file no longer exists)","este trabajo no se puede reintentar (no falló o su archivo ya no existe)"],["a folder cannot be moved into itself or into a folder that does not exist","una carpeta no se puede mover dentro de sí misma, ni a una que no existe"],["this is the only administrator; make someone else an administrator first","es el único administrador; nombra otro antes"],["only the uploader or an administrator can change a document's privacy","solo quien subió el documento o un administrador puede cambiar su privacidad"],["a folder name must be 1 to 80 characters long and cannot contain \"/\"","el nombre de la carpeta debe tener entre 1 y 80 caracteres y no llevar «/»"],["this document's file is not on disk; check Settings → Health","el archivo de este documento no está en disco; revisa Ajustes → Salud"],["too many attempts from your connection; wait a few minutes","demasiados intentos desde tu conexión; espera unos minutos"],["the copy does not match the original (different SHA-256)","la copia no coincide con el original (SHA-256 distinto)"],["a folder with that name already exists here","ya hay una carpeta con ese nombre en ese lugar"],["the password must be at least 8 characters long","la contraseña debe tener al menos 8 caracteres"],["the model's response was truncated (max_tokens)","la respuesta del modelo quedó truncada (max_tokens)"],[".{} requires PAPIRO_GOTENBERG_URL to be converted",".{} requiere PAPIRO_GOTENBERG_URL para convertirse"],["only an administrator can delete permanently","solo un administrador puede borrar definitivamente"],["a read-only token cannot change the password","un token de solo lectura no puede cambiar la contraseña"],["a read-only token cannot create other tokens","un token de solo lectura no puede crear otros tokens"],["the URL must start with http:// or https://","la URL debe empezar con http:// o https://"],["the model declined to process this document","el modelo declinó procesar este documento"],["action must be accept, dismiss or refresh","action debe ser accept, dismiss o refresh"],["no LLM is configured (PAPIRO_LLM_API_KEY)","no hay LLM configurado (PAPIRO_LLM_API_KEY)"],["server busy; try again in a few seconds","servidor ocupado; intenta de nuevo en unos segundos"],["{} document(s) moved to the parent folder","{} documento(s) pasaron a la carpeta superior"],["the original does not match its SHA-256","el original no coincide con su SHA-256"],["a read-only token cannot revoke tokens","un token de solo lectura no puede revocar tokens"],["invalid role (admin, editor or viewer)","rol inválido (admin, editor o viewer)"],["the original is {} bytes but should be {}","el original mide {} bytes y deberían ser {}"],["permanently deleted from the console","borrado definitivo desde la consola"],["this installation already has users","la instalación ya tiene usuarios"],["the LLM did not return valid JSON: {}","el LLM no devolvió JSON válido: {}"],["the LLM response contains no text","respuesta del LLM sin texto"],["the parent folder does not exist","la carpeta superior no existe"],["you cannot delete your own user","no puedes borrar tu propio usuario"],["your user or token is read-only","tu usuario o token es de solo lectura"],["administrator account created","cuenta de administrador creada"],["could not copy the database: {}","no se pudo copiar la base: {}"],["the job is still in progress","el trabajo sigue en curso"],["unreadable LLM response ({}): {}","respuesta ilegible del LLM ({}): {}"],["document permanently deleted","documento borrado definitivamente"],["more than {} days in the trash","más de {} días en la papelera"],["the original file is missing","falta el archivo original"],["document classified with AI","documento clasificado con IA"],["that folder does not exist","esa carpeta no existe"],["expires must be YYYY-MM-DD","expires debe ser YYYY-MM-DD"],["created must be YYYY-MM-DD","created debe ser YYYY-MM-DD"],["wrong username or password","usuario o contraseña incorrectos"],["the archive PDF is missing","falta el PDF de archivo"],["could not render the page","no se pudo renderizar la página"],["the file no longer exists","el archivo ya no existe"],["the username is required","falta el nombre de usuario"],["duplicate of document #{}","duplicado del documento #{}"],["unsupported file type: {}","tipo de archivo no soportado: {}"],["document moved to trash","documento a la papelera"],["document uploaded (MCP)","documento subido (MCP)"],["administrator required","requiere administrador"],["correspondent deleted","borrado en corresponsales"],["document type deleted","borrado en tipos de documento"],["document edited (MCP)","documento editado (MCP)"],["missing \"file\" field","falta el campo «file»"],["could not resolve {}: {}","no se pudo resolver {}: {}"],["no LLM is configured","LLM no configurado"],["suggestions accepted","sugerencias aceptadas"],["failed job dismissed","trabajo fallido descartado"],["the URL has no host","URL sin host"],["no disk space left","no queda espacio en disco"],["origin not allowed","origen no permitido"],["\"{}\" already exists","ya existe «{}»"],["not authenticated","no autenticado"],["LLM unreachable: {}","LLM inalcanzable: {}"],["document uploaded","documento subido"],["document restored","documento restaurado"],["name is required","falta name"],["unknown action: {}","acción desconocida: {}"],["LLM responded {}: {}","LLM respondió {}: {}"],["could not run {}: {}","no se pudo ejecutar {}: {}"],["{}: timed out ({} s)","{}: tiempo agotado ({} s)"],["password changed","contraseña cambiada"],["document edited","documento editado"],["integrity check","verificación de integridad"],["webhook created","webhook creado"],["webhook deleted","webhook borrado"],["folder created","carpeta creada"],["folder deleted","carpeta borrada"],["password reset","contraseña restablecida"],["token created","token creado"],["token revoked","token revocado"],["trash emptied","papelera vaciada"],["bulk edit ({})","edición masiva ({})"],["user created","usuario creado"],["role changed","rol cambiado"],["user deleted","usuario borrado"],["login failed","inicio de sesión fallido"],["tag deleted","borrado en etiquetas"],["conflict: {}","conflicto: {}"],["no details","sin detalle"],["{} documents","{} documentos"],["not found","no encontrado"],["bulk edit","edición masiva"],["logged in","sesión iniciada"],["document {}","documento {}"],["{} problems","{} problemas"],["read-only","solo lectura"],["blocked: {}","bloqueado: {}"],["console","consola"],["unknown","desconocida"],["system","sistema"]];
const SERVER_RE = SERVER_ES.map(([pat, es]) => [new RegExp(`^${pat.split('{}').map((p) => p.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('(.+?)')}$`, 's'), es]);
function srv(m) {
  if (LANG !== 'es' || typeof m !== 'string') return m;
  for (const [re, es] of SERVER_RE) { const x = m.match(re); if (x) { let i = 1; return es.replace(/\{\}/g, () => x[i++] ?? ''); } }
  return m;
}

// ───────────── utilidades ─────────────
const ICONS = {
  inbox: 'M22 12h-6l-2 3h-4l-2-3H2M5.5 5.1 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.5-6.9A2 2 0 0 0 16.8 4H7.2a2 2 0 0 0-1.7 1.1z',
  files: 'M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7zM14 2v5h6M9 13h6M9 17h6',
  trash: 'M3 6h18M8 6V4h8v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6',
  search: 'M21 21l-4.3-4.3M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16z',
  upload: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12',
  settings: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.8 1.2V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-2.8-1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0-1.2-2.8H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.2-2.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 2.8-1.2V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 2.8 1.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0 1.2 2.8h.1a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z',
  x: 'M18 6 6 18M6 6l12 12', check: 'M20 6 9 17l-5-5', plus: 'M12 5v14M5 12h14', back: 'M19 12H5M12 19l-7-7 7-7',
  down: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3',
  moon: 'M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z', sun: 'M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4',
  tag: 'M20.6 13.4 13.4 20.6a2 2 0 0 1-2.8 0L2 12V2h10l8.6 8.6a2 2 0 0 1 0 2.8zM7 7h.01',
  user: 'M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z',
  type: 'M4 4h16v16H4zM9 9h6M9 13h6M9 17h3', grid: 'M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z', rows: 'M3 6h18M3 12h18M3 18h18',
  spark: 'M12 3l1.9 5.6L19.5 10l-5.6 1.9L12 17.5l-1.9-5.6L4.5 10l5.6-1.4zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z',
  key: 'M21 2l-2 2m-7.6 7.6a5.5 5.5 0 1 1-7.8 7.8 5.5 5.5 0 0 1 7.8-7.8zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4',
  hook: 'M18 16.98h-5.99c-1.1 0-1.95.94-2.48 1.9A4 4 0 0 1 2 17c.01-.7.2-1.4.57-2M6 17l3.13-5.78c.53-.97.1-2.18-.5-3.1a4 4 0 1 1 6.89-4.06M12 6l3.13 5.73C15.66 12.7 16.9 13 18 13a4 4 0 0 1 0 8',
  menu: 'M3 6h18M3 12h18M3 18h18', help: 'M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z', code: 'M16 18l6-6-6-6M8 6l-6 6 6 6',
  folder: 'M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9l-.8-1.2A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z', chev: 'M9 18l6-6-6-6',
  dots: 'M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM19 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM5 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2z', edit: 'M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z',
  camera: 'M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3zM12 16a3 3 0 1 0 0-6 3 3 0 0 0 0 6z', ext: 'M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6',
  lock: 'M7 11V7a5 5 0 0 1 10 0v4M6 11h12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1z', cal: 'M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z',
  install: 'M12 3v12M7 10l5 5 5-5M5 21h14', clock: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 6v6l4 2', cash: 'M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6',
  restore: 'M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5', logout: 'M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9', activity: 'M22 12h-4l-3 9L9 3l-3 9H2',
  globe: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z',
};
const Icon = ({ n }) => html`<svg class="i" viewBox="0 0 24 24"><path d=${ICONS[n]} /></svg>`;
const fmtDate = (d) => d ? new Date(d.length === 10 ? d + 'T12:00' : d.replace(' ', 'T') + 'Z').toLocaleDateString(LOCALE, { day: 'numeric', month: 'short', year: 'numeric' }) : '';
const fmtSize = (b) => b > 1e6 ? (b / 1048576).toFixed(1) + ' MB' : Math.max(1, Math.round(b / 1024)) + ' KB';
const cls = (...a) => a.filter(Boolean).join(' ');
// The currency is unknown: a plain number with 2 decimals.
const fmtMoney = (n) => n.toLocaleString(LOCALE, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
/** «Vencido», «Vence en 12 días»… o null si falta más de 60 días. */
function dueLabel(d) {
  const n = Math.round((new Date(d + 'T12:00') - new Date(new Date().toDateString() + ' 12:00')) / 864e5);
  return n < 0 ? ['bad', t('Overdue')] : n === 0 ? ['bad', t('Due today')] : n <= 60 ? ['warn', tn(n, 'Due in {n} day', 'Due in {n} days')] : null;
}
const debounce = (fn, ms) => { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); }; };

// ───────────── estado global ─────────────
const parseHash = () => {
  const p = new URLSearchParams(location.hash.replace(/^#\/?\??/, ''));
  const num = (k) => (p.get(k) ? Number(p.get(k)) : null);
  return { view: p.get('view') || 'all', q: p.get('q') || '', tags: (p.get('tags') || '').split(',').filter(Boolean).map(Number), c: num('c'), t: num('t'), f: num('f'),
    from: p.get('from') || '', to: p.get('to') || '', doc: num('doc'), sort: p.get('sort') || '' };
};
/** Lo que se limpia al cambiar de vista. */
const HOME = { doc: null, tags: [], c: null, t: null, q: '', from: '', to: '', f: null };
const touch = matchMedia('(hover: none)').matches;
const store = {
  state: { ready: false, setup: false, user: null, llm: false, labels: { tag: [], correspondent: [], doctype: [] }, folders: [], meta: false, stats: {}, route: parseHash(), docs: [], total: 0,
    loading: false, amountTotal: null, cur: 0, picked: new Set(), jobs: [], toasts: [], modal: null, palette: false, grid: localStorage.grid === '1', menu: false, jobsOpen: false,
    open: new Set(JSON.parse(localStorage.open || '[]')), dialog: null, sheet: null, install: null, online: navigator.onLine },
  subs: new Set(),
  set(patch) { Object.assign(this.state, typeof patch === 'function' ? patch(this.state) : patch); this.subs.forEach((f) => f()); },
};
const S = () => store.state;
function useStore() {
  const [, tick] = useState(0);
  useEffect(() => { const f = () => tick((x) => x + 1); store.subs.add(f); return () => store.subs.delete(f); }, []);
  return store.state;
}
function nav(patch, replace) {
  const r = { ...S().route, ...patch }, p = new URLSearchParams();
  if (r.view !== 'all') { p.set('view', r.view); r.f = null; }
  for (const k of ['q', 'c', 't', 'f', 'from', 'to', 'doc', 'sort']) if (r[k]) p.set(k, r[k]);
  if (r.tags.length) p.set('tags', r.tags.join(','));
  const h = '#/' + (p.toString() ? '?' + p : '');
  replace ? history.replaceState(null, '', h) : history.pushState(null, '', h);
  onRoute();
}
function onRoute() {
  const prev = S().route, route = parseHash();
  store.set({ route, menu: false });
  if (['view', 'q', 'tags', 'c', 't', 'f', 'from', 'to', 'sort'].some((k) => String(prev[k]) !== String(route[k]))) { store.set({ picked: new Set(), cur: 0 }); loadDocs(); }
}
addEventListener('hashchange', onRoute); addEventListener('popstate', onRoute);

async function api(path, o = {}) {
  const init = { method: o.method || (o.json !== undefined || o.body ? 'POST' : 'GET'), headers: { 'X-Requested-With': 'papiro' }, credentials: 'same-origin' };
  if (o.json !== undefined) { init.headers['Content-Type'] = 'application/json'; init.body = JSON.stringify(o.json); }
  if (o.body) init.body = o.body;
  const r = await fetch('/api' + path, init).catch(() => { throw new Error(o.body && navigator.onLine ? t('The upload was cut off before it finished. Please try again.') : t('No connection to Papiro')); });
  if (r.status === 401 && !o.quiet) store.set({ user: null });
  const data = r.status === 204 ? null : await r.json().catch(() => null);
  if (!r.ok) throw new Error(srv(data?.error) || `Error ${r.status}`);
  return data;
}
function toast(text, kind = 'ok', ms = 4200) {
  const id = Math.random();
  store.set((s) => ({ toasts: [...s.toasts, { id, text, kind }] }));
  setTimeout(() => store.set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })), ms);
}
const attempt = (p, okMsg) => p.then((v) => { okMsg && toast(okMsg); return v; }).catch((e) => { toast(e.message, 'bad', 6500); });
/** Diálogo propio en lugar de confirm()/prompt(): devuelve true, el texto escrito o null si se cancela. */
const ask = (d) => new Promise((resolve) => store.set({ dialog: { ...d, resolve, id: Math.random() } }));

const PAGE = 60;
let seq = 0;
async function loadDocs(more) {
  const { route: r, docs } = S(), my = ++seq, p = new URLSearchParams({ limit: PAGE, offset: more ? docs.length : 0 });
  if (r.q) p.set('q', r.q);
  if (r.tags.length) p.set('tags', r.tags.join(','));
  if (r.c) p.set('correspondent', r.c);
  if (r.t) p.set('doctype', r.t);
  if (r.f && r.view === 'all') p.set('folder', r.f);
  if (r.from) p.set('from', r.from);
  if (r.to) p.set('to', r.to);
  if (r.view === 'inbox') p.set('inbox', 'true');
  if (r.view === 'trash') p.set('trash', 'true');
  if (r.view === 'expiring') p.set('expiring', 'true');
  if (r.sort) { const [s, o] = r.sort.split(':'); p.set('sort', s); p.set('order', o || 'desc'); }
  store.set({ loading: true });
  try {
    const res = await api('/documents?' + p);
    if (my === seq) store.set({ docs: more ? [...docs, ...res.items] : res.items, total: res.total, amountTotal: res.amount_total, loading: false });
  } catch { if (my === seq) store.set({ loading: false }); }
}
async function loadMeta() {
  const [tag, correspondent, doctype, folders, stats] = await Promise.all([api('/tags'), api('/correspondents'), api('/doctypes'), api('/folders'), api('/stats')]);
  store.set({ labels: { tag, correspondent, doctype }, folders, stats, meta: true });
}
const refresh = debounce(() => { loadDocs(); loadMeta(); }, 250);
const labelOf = (kind, id) => S().labels[kind].find((l) => l.id === id);
const folderOf = (id) => (id ? S().folders.find((f) => f.id === id) : undefined);
const kids = (id) => S().folders.filter((f) => (f.parent_id ?? null) === (id ?? null));
const here = () => { const r = S().route; return r.view === 'all' ? folderOf(r.f) : undefined; };
/** Sugerencias del análisis local: `pend` son las que esperan respuesta; `auto`, las que se aplicaron solas. */
function suggestions(d) {
  const s = d.suggest || {}, pend = [], auto = [];
  const add = (key, icon, label, kind, x) => {
    const name = kind === 'folder' ? folderOf(x.value)?.path.replaceAll('/', ' › ') : labelOf(kind, x.value)?.name;
    if (name) (x.applied ? auto : pend).push({ key, icon, label, name, similar: x.similar });
  };
  if (s.folder_id) add('folder_id', 'folder', t('Folder'), 'folder', s.folder_id);
  if (s.doctype_id) add('doctype_id', 'type', t('Type'), 'doctype', s.doctype_id);
  if (s.correspondent_id) add('correspondent_id', 'user', t('Correspondent'), 'correspondent', s.correspondent_id);
  for (const x of s.tags || []) add(`tag:${x.value}`, 'tag', t('Tag'), 'tag', x);
  return { pend, auto };
}

async function boot() {
  // Sin conexión (o con el servidor caído) la app instalada abre igual y lo dice, en vez de quedarse cargando.
  const st = await api('/auth/status', { quiet: true }).catch(() => null);
  if (!st) return store.set({ ready: true, down: true });
  store.set({ ready: true, down: false, setup: st.setup_required, user: st.user, llm: st.llm });
  if (st.user) start();
}
let es;
function start() {
  loadMeta(); loadDocs();
  api('/jobs').then((jobs) => store.set({ jobs }));
  es?.close();
  es = new EventSource('/api/events');
  es.onmessage = (m) => {
    const ev = JSON.parse(m.data);
    if (ev.type === 'job') {
      const j = ev.job;
      store.set((s) => ({ jobs: [j, ...s.jobs.filter((x) => x.id !== j.id)].slice(0, 100) }));
      if (j.status === 'failed') toast(`${j.filename}: ${srv(j.error)}`, 'bad', 9000);
      if (j.status === 'duplicate') toast(t('{file} was already in the archive', { file: j.filename }), 'warn');
    } else { refresh(); window.dispatchEvent(new CustomEvent('doc-changed', { detail: ev })); }
  };
  takeShared();
  if (location.hash.includes('focus=1')) setTimeout(() => document.getElementById('q')?.focus(), 300);
}

async function upload(files) {
  files = [...files];
  if (!files.length) return;
  if (!canWrite()) return toast(t('Your user is read-only'), 'warn');
  const folder = here(), v = { name: files[0].name || t('document'), n: files.length, folder: folder?.name };
  toast(files.length === 1 ? t(folder ? 'Uploading {name} to “{folder}”…' : 'Uploading {name}…', v) : t(folder ? 'Uploading {n} files to “{folder}”…' : 'Uploading {n} files…', v));
  // De tres en tres: el servidor los encola y procesa a su ritmo.
  const queue = files.slice();
  await Promise.all([0, 1, 2].map(async () => {
    for (let f; (f = queue.shift());) {
      const name = f.name || `${t('document')}.${(f.type.split('/')[1] || 'bin').replace('jpeg', 'jpg')}`;
      // Un archivo arrastrado desde otra aplicación (o movido tras elegirlo) puede no ser legible: se avisa antes de enviar nada.
      if (!f.size || !await f.slice(0, 1).arrayBuffer().then(() => true, () => false)) { toast(t('The browser couldn’t read “{name}”. Try the Upload button or copy the file to another folder.', { name }), 'bad', 9000); continue; }
      const body = new FormData(); body.append('file', f, name); if (folder) body.append('folder_id', folder.id); await attempt(api('/documents', { body }));
    }
  }));
}
/** Abre el selector de archivos; con `camera`, la cámara del teléfono. */
function pickFiles(camera) {
  const i = Object.assign(document.createElement('input'), { type: 'file', multiple: !camera });
  if (camera) { i.accept = 'image/*'; i.capture = 'environment'; }
  i.onchange = () => upload(i.files); i.click();
}
const addDocs = () => (touch ? store.set({ sheet: 'upload', menu: false }) : pickFiles());
async function bulk(ids, json, msg) { await attempt(api('/documents/bulk', { json: { ids: [...ids], ...json } }), msg); store.set({ picked: new Set() }); refresh(); }
function setTheme(t) { t ? (localStorage.theme = t) : delete localStorage.theme; document.documentElement.dataset.theme = t || ''; themeColor(); store.set({}); }
/** La barra del sistema (y la del navegador en móvil) toma el color de fondo de la app. */
function themeColor() { const bg = getComputedStyle(document.documentElement).getPropertyValue('--bg').trim(); document.querySelectorAll('meta[name=theme-color]').forEach((m) => { m.content = bg; m.removeAttribute('media'); }); }
if (localStorage.theme) { document.documentElement.dataset.theme = localStorage.theme; themeColor(); }
const isDark = () => (document.documentElement.dataset.theme || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')) === 'dark';
const inboxIds = () => S().labels.tag.filter((t) => t.is_inbox).map((t) => t.id);
const canWrite = () => S().user?.role !== 'viewer';
const isAdmin = () => S().user?.role === 'admin';
const openJobs = () => { api('/jobs').then((jobs) => store.set({ jobs })); store.set({ jobsOpen: true, menu: false }); };

// carpetas
function toggleOpen(id, on) { const open = new Set(S().open); (on ?? !open.has(id)) ? open.add(id) : open.delete(id); localStorage.open = JSON.stringify([...open]); store.set({ open }); }
async function newFolder(parent_id = null) {
  const parent = folderOf(parent_id);
  const name = await ask({ title: parent ? t('New folder in “{name}”', { name: parent.name }) : t('New folder'), input: true, placeholder: t('Folder name'), ok: t('Create') });
  const f = name && await attempt(api('/folders', { json: { name, parent_id } }));
  if (f) { await loadMeta(); parent_id && toggleOpen(parent_id, true); }
  return f;
}
async function renameFolder(f) {
  const name = await ask({ title: t('Rename'), input: true, value: f.name, ok: t('Save') });
  if (name && name !== f.name) { await attempt(api(`/folders/${f.id}`, { method: 'PATCH', json: { name } })); loadMeta(); }
}
async function deleteFolder(f) {
  const sub = kids(f.id).length, parent = folderOf(f.parent_id);
  const text = (parent ? t('Its documents aren’t deleted: they move to “{name}”.', { name: parent.name }) : t('Its documents aren’t deleted: they’re left without a folder.')) + (sub ? ' ' + t('Its subfolders are deleted too.') : '');
  if (!await ask({ title: t('Delete the folder “{name}”?', { name: f.name }), text, ok: t('Delete folder'), danger: true })) return;
  await attempt(api(`/folders/${f.id}`, { method: 'DELETE' }), t('Folder deleted'));
  if (S().route.f === f.id) nav({ f: f.parent_id || null }, true);
  refresh();
}
const folderMenu = (f) => [['folder', t('New subfolder'), () => newFolder(f.id)], ['edit', t('Rename'), () => renameFolder(f)], ['trash', t('Delete folder'), () => deleteFolder(f), true]];
const moveTo = (ids, folder_id) => bulk(ids, { folder_id }, folder_id ? t('Moved to “{name}”', { name: folderOf(folder_id)?.name }) : t('Removed from its folder'));
/** Arrastrar documentos de la lista y soltarlos sobre una carpeta los mueve. */
const DND = 'application/x-papiro-docs';
const dropOn = (folder_id) => canWrite() && {
  onDragOver: (e) => { if ([...e.dataTransfer.types].includes(DND)) { e.preventDefault(); e.currentTarget.classList.add('drop-on'); } },
  onDragLeave: (e) => e.currentTarget.classList.remove('drop-on'),
  onDrop: (e) => { e.currentTarget.classList.remove('drop-on'); const ids = e.dataTransfer.getData(DND); if (ids) { e.preventDefault(); moveTo(ids.split(',').map(Number), folder_id); } },
};

// aplicación instalable
if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {});
addEventListener('beforeinstallprompt', (e) => { e.preventDefault(); store.set({ install: e }); });
addEventListener('appinstalled', () => store.set({ install: null }));
addEventListener('online', () => { store.set({ online: true }); S().down ? boot() : S().user && refresh(); });
addEventListener('offline', () => store.set({ online: false }));
/** Archivos que llegaron con «Compartir → Papiro»: el service worker los guardó y aquí se suben. */
async function takeShared() {
  if (!('caches' in window)) return;
  const cache = await caches.open('papiro-share'), files = [];
  for (const k of await cache.keys()) {
    const r = await cache.match(k);
    files.push(new File([await r.blob()], decodeURIComponent(r.headers.get('X-Name') || t('document')), { type: r.headers.get('Content-Type') || '' }));
    await cache.delete(k);
  }
  upload(files);
}

// ───────────── piezas comunes ─────────────
const Tag = ({ l, onRemove }) => html`<span class="tag" style=${`--c:${l.color}`}>${l.name}${onRemove && html`<button onClick=${(e) => { e.stopPropagation(); onRemove(); }} title=${t('Remove')}>×</button>`}</span>`;

/** Cierra un desplegable al tocar fuera de él. */
function usePop(onClose) {
  const ref = useRef();
  useEffect(() => {
    const out = (e) => ref.current && !ref.current.contains(e.target) && onClose();
    const t = setTimeout(() => addEventListener('pointerdown', out));
    return () => { clearTimeout(t); removeEventListener('pointerdown', out); };
  }, []);
  return ref;
}
const listKeys = (n, hi, setHi, choose, onClose) => (e) => {
  if (e.key === 'ArrowDown') { setHi((hi + 1) % Math.max(n, 1)); e.preventDefault(); }
  else if (e.key === 'ArrowUp') { setHi((hi - 1 + n) % Math.max(n, 1)); e.preventDefault(); }
  else if (e.key === 'Enter') { choose(hi); e.preventDefault(); }
  else if (e.key === 'Escape') { onClose(); e.stopPropagation(); }
};

/** Selector con filtro y «crear»: sirve para etiquetas (multi), corresponsal y tipo. */
function Picker({ kind, value, multi, onChange, onClose }) {
  const s = useStore(), [q, setQ] = useState(''), [hi, setHi] = useState(0), input = useRef(), ref = usePop(onClose);
  const sel = new Set(multi ? value : value ? [value] : []);
  const list = s.labels[kind].filter((l) => l.name.toLowerCase().includes(q.trim().toLowerCase()));
  const canCreate = canWrite() && q.trim() && !s.labels[kind].some((l) => l.name.toLowerCase() === q.trim().toLowerCase());
  const n = list.length + (canCreate ? 1 : 0);
  useEffect(() => { input.current.focus(); }, []);
  const choose = async (i) => {
    let l = list[i];
    if (!l && canCreate) { l = await attempt(api('/' + ({ tag: 'tags', correspondent: 'correspondents', doctype: 'doctypes' })[kind], { json: { name: q.trim() } })); if (!l) return; await loadMeta(); }
    if (!l) return;
    if (multi) { sel.has(l.id) ? sel.delete(l.id) : sel.add(l.id); onChange([...sel], l); setQ(''); } else { onChange(sel.has(l.id) ? null : l.id, l); onClose(); }
  };
  return html`<div class="pop-scrim" /><div class="pop" ref=${ref} onClick=${(e) => e.stopPropagation()}>
    <input ref=${input} value=${q} placeholder=${canWrite() ? t('Search or create…') : t('Search…')} onInput=${(e) => { setQ(e.target.value); setHi(0); }} onKeyDown=${listKeys(n, hi, setHi, choose, onClose)} />
    <div class="opts scroll">
      ${list.map((l, i) => html`<button class=${cls('opt', i === hi && 'hi')} onMouseEnter=${() => setHi(i)} onClick=${() => choose(i)}>
        <i class="dot" style=${`background:${l.color}`} /><span>${l.name}</span>${sel.has(l.id) && html`<span class="tick"><${Icon} n="check" /></span>`}</button>`)}
      ${canCreate && html`<button class=${cls('opt', hi === list.length && 'hi')} onMouseEnter=${() => setHi(list.length)} onClick=${() => choose(list.length)}><${Icon} n="plus" /><span>${t('Create “{name}”', { name: q.trim() })}</span></button>`}
      ${!n && html`<div class="opt muted">${t('No results')}</div>`}
    </div></div>`;
}

/** Selector de carpeta: filtra por ruta, permite sacar el documento de su carpeta y crear una nueva. */
function FolderPicker({ value, onChange, onClose }) {
  const s = useStore(), [q, setQ] = useState(''), [hi, setHi] = useState(0), input = useRef(), ref = usePop(onClose), needle = q.trim().toLowerCase();
  const list = s.folders.filter((f) => f.path.toLowerCase().includes(needle));
  const items = [...(value && !needle ? [{ id: null, none: true }] : []), ...list];
  const canCreate = needle && !q.includes('/') && !s.folders.some((f) => !f.parent_id && f.name.toLowerCase() === needle);
  const n = items.length + (canCreate ? 1 : 0);
  useEffect(() => { input.current.focus(); }, []);
  const choose = async (i) => {
    let f = items[i];
    if (!f && canCreate) { f = await attempt(api('/folders', { json: { name: q.trim() } })); if (!f) return; await loadMeta(); }
    if (f) { onChange(f.id, f); onClose(); }
  };
  return html`<div class="pop-scrim" /><div class="pop" ref=${ref} onClick=${(e) => e.stopPropagation()}>
    <input ref=${input} value=${q} placeholder=${t('Search or create a folder…')} onInput=${(e) => { setQ(e.target.value); setHi(0); }} onKeyDown=${listKeys(n, hi, setHi, choose, onClose)} />
    <div class="opts scroll">
      ${items.map((f, i) => html`<button class=${cls('opt fold', i === hi && 'hi')} onMouseEnter=${() => setHi(i)} onClick=${() => choose(i)}>
        ${f.none ? html`<${Icon} n="x" /><span>${t('No folder')}</span>` : html`<${Icon} n="folder" /><span>${f.path.replaceAll('/', ' › ')}</span>${f.id === value && html`<span class="tick"><${Icon} n="check" /></span>`}`}</button>`)}
      ${canCreate && html`<button class=${cls('opt', hi === items.length && 'hi')} onMouseEnter=${() => setHi(items.length)} onClick=${() => choose(items.length)}><${Icon} n="plus" /><span>${t('Create folder “{name}”', { name: q.trim() })}</span></button>`}
      ${!n && html`<div class="opt muted">${t('No folders yet: type a name to create the first one')}</div>`}
    </div></div>`;
}

/** Menú de acciones: [icono, texto, acción, peligrosa]. */
function Menu({ items, onClose, right }) {
  const ref = usePop(onClose);
  return html`<div class="pop-scrim" /><div class=${cls('pop menu', right && 'right')} ref=${ref} onClick=${(e) => e.stopPropagation()}>${items.filter(Boolean).map(([icon, label, fn, danger]) =>
    html`<button class=${cls('opt', danger && 'danger')} onClick=${() => { onClose(); fn(); }}><${Icon} n=${icon} /><span>${label}</span></button>`)}</div>`;
}
/** Botón que abre un menú. */
function MenuButton({ items, icon = 'dots', title, right, cls: c = 'btn icon ghost sm', children }) {
  const [open, setOpen] = useState(false);
  return html`<span class="rel"><button class=${c} title=${title} onClick=${(e) => { e.stopPropagation(); setOpen(!open); }}><${Icon} n=${icon} />${children}</button>
    ${open && html`<${Menu} items=${items} right=${right} onClose=${() => setOpen(false)} />`}</span>`;
}
function Dialog({ d }) {
  const [v, setV] = useState(d.value || '');
  const done = (r) => { store.set({ dialog: null }); d.resolve(r); };
  return html`<div class="veil center" onMouseDown=${(e) => e.target === e.currentTarget && done(null)}><form class="dialog" onSubmit=${(e) => { e.preventDefault(); done(d.input ? v.trim() || null : true); }}>
    <h3>${d.title}</h3>${d.text && html`<p>${d.text}</p>`}
    ${d.input && html`<input class="input" ref=${(el) => el && !el.dataset.f && (el.dataset.f = 1, el.focus(), el.select())} type=${d.type || 'text'} minlength=${d.minlength} value=${v} placeholder=${d.placeholder || ''} onInput=${(e) => setV(e.target.value)} required />`}
    <div class="row end"><button type="button" class="btn" onClick=${() => done(null)}>${t('Cancel')}</button><button class=${cls('btn primary', d.danger && 'danger')}>${d.ok || t('OK')}</button></div></form></div>`;
}

// ───────────── acceso ─────────────
function Auth({ setup }) {
  const [u, setU] = useState(''), [p, setP] = useState(''), [err, setErr] = useState(''), [busy, setBusy] = useState(false);
  const go = async (e) => {
    e.preventDefault(); setBusy(true); setErr('');
    try { const user = await api(setup ? '/auth/setup' : '/auth/login', { json: { username: u, password: p }, quiet: true }); store.set({ user, setup: false }); start(); }
    catch (x) { setErr(x.message); } setBusy(false);
  };
  return html`<div class="auth"><form onSubmit=${go}>
    <div class="brand"><img class="logo" src="/assets/icon.svg" alt="" />Papiro</div>
    <h1>${setup ? t('Create your admin account') : t('Sign in')}</h1>
    ${setup && html`<div class="muted">${t('This is the first time this installation has been opened. This account will have full control.')}</div>`}
    <label class="field">${t('Username')}<input class="input" value=${u} onInput=${(e) => setU(e.target.value)} autocomplete="username" autofocus required /></label>
    <label class="field">${t('Password')}<input class="input" type="password" value=${p} onInput=${(e) => setP(e.target.value)} autocomplete=${setup ? 'new-password' : 'current-password'} minlength=${setup ? 8 : 1} required /></label>
    ${err && html`<div class="err">${err}</div>`}
    <button class="btn primary big" disabled=${busy}>${setup ? t('Create account') : t('Log in')}</button>
  </form></div>`;
}


// ───────────── barra lateral ─────────────
function FolderNode({ f, depth }) {
  const s = S(), sub = kids(f.id), on = here()?.id === f.id, open = s.open.has(f.id), [menu, setMenu] = useState(false);
  return html`<div class="rel"><div class=${cls('nav folder', on && 'on')} style=${`padding-left:${10 + depth * 16}px`} role="button" tabindex="0" ...${dropOn(f.id)}
      onClick=${() => { nav({ ...HOME, view: 'all', f: f.id }); sub.length && toggleOpen(f.id, true); }} onKeyDown=${(e) => e.key === 'Enter' && e.target === e.currentTarget && nav({ ...HOME, view: 'all', f: f.id })}>
      <button class=${cls('twist', open && 'open', !sub.length && 'none')} title=${open ? t('Collapse') : t('Expand')} onClick=${(e) => { e.stopPropagation(); toggleOpen(f.id); }}><svg class="i t" viewBox="0 0 24 24"><path d=${ICONS.chev} /></svg></button>
      <${Icon} n="folder" /><span class="name">${f.name}</span><span class="count">${f.document_count || ''}</span>
      ${canWrite() && html`<button class="more" title=${t('Folder options')} onClick=${(e) => { e.stopPropagation(); setMenu(!menu); }}><${Icon} n="dots" /></button>`}</div>
    ${menu && html`<${Menu} items=${folderMenu(f)} onClose=${() => setMenu(false)} />`}
    ${open && sub.map((c) => html`<${FolderNode} f=${c} depth=${depth + 1} key=${c.id} />`)}</div>`;
}
function Sidebar() {
  const s = useStore(), r = s.route, active = s.jobs.filter((j) => j.status === 'queued' || j.status === 'running').length, roots = kids(null);
  const group = (title, kind, key, icon) => {
    const items = s.labels[kind].filter((l) => l.document_count > 0 || kind === 'tag');
    return items.length > 0 && html`<details open=${kind === 'tag' || items.length <= 6}><summary>${title}</summary>${items.map((l) => {
      const on = kind === 'tag' ? r.tags.includes(l.id) : r[key] === l.id;
      const go = () => nav(kind === 'tag' ? { tags: on ? r.tags.filter((x) => x !== l.id) : [...r.tags, l.id], doc: null } : { [key]: on ? null : l.id, doc: null });
      return html`<button class=${cls('nav', on && 'on')} onClick=${go}>${kind === 'tag' ? html`<i class="dot" style=${`background:${l.color}`} />` : html`<${Icon} n=${icon} />`}
        <span class="name">${l.name}</span><span class="count">${l.document_count || ''}</span></button>`;
    })}</details>`;
  };
  const view = (v, icon, label, count) => html`<button class=${cls('nav', r.view === v && !here() && 'on')} onClick=${() => nav({ ...HOME, view: v })} ...${v === 'all' ? dropOn(null) : {}}>
    <${Icon} n=${icon} /><span class="name">${label}</span><span class="count">${count || ''}</span></button>`;
  return html`<nav class=${cls('side', s.menu && 'open')}>
    <div class="side-head"><div class="brand"><img class="logo" src="/assets/icon.svg" alt="" />Papiro</div>
      <button class="btn icon ghost only-narrow" title=${t('Close')} onClick=${() => store.set({ menu: false })}><${Icon} n="x" /></button></div>
    ${canWrite() && html`<button class="btn primary big only-wide" onClick=${() => pickFiles()} title=${t('Upload documents (U)')}><${Icon} n="upload" />${t('Upload documents')}<kbd>U</kbd></button>`}
    <div class="side-body scroll">
      ${view('inbox', 'inbox', t('Inbox'), s.stats.inbox)}${view('all', 'files', t('All documents'), s.stats.documents)}
      ${(s.stats.expiring > 0 || r.view === 'expiring') && view('expiring', 'clock', t('Due dates'), s.stats.expiring)}${view('trash', 'trash', t('Trash'), s.stats.trash)}
      <h4>${t('Folders')}${canWrite() && html`<button title=${t('New folder')} onClick=${() => newFolder()}><${Icon} n="plus" /></button>`}</h4>
      ${roots.map((f) => html`<${FolderNode} f=${f} depth=${0} key=${f.id} />`)}
      ${!roots.length && s.meta && html`<button class="nav hint" disabled=${!canWrite()} onClick=${() => newFolder()}><${Icon} n="folder" /><span class="name">${canWrite() ? t('Create your first folder') : t('No folders yet')}</span></button>`}
      ${group(t('Tags'), 'tag')}${group(t('Correspondents'), 'correspondent', 'c', 'user')}${group(t('Types'), 'doctype', 't', 'type')}
    </div>
    <div class="side-foot">
      <button class="nav only-wide" onClick=${openJobs}>${active ? html`<span class="spinner" />` : html`<${Icon} n="activity" />`}
        <span class="name">${active ? t('Processing {n}…', { n: active }) : t('Activity')}</span>${s.stats.failed > 0 && html`<span class="count" style="color:var(--danger)" title=${t('Failed jobs')}>${s.stats.failed}</span>`}</button>
      ${s.install && html`<button class="nav" onClick=${() => { s.install.prompt(); store.set({ install: null, menu: false }); }}><${Icon} n="install" /><span class="name">${t('Install Papiro')}</span></button>`}
      <button class="nav" onClick=${() => store.set({ modal: 'settings', menu: false })}><${Icon} n="settings" /><span class="name">${t('Settings')}</span><span class="count only-wide"><kbd>,</kbd></span></button>
      <button class="nav" onClick=${() => setTheme(isDark() ? 'light' : 'dark')}><${Icon} n=${isDark() ? 'sun' : 'moon'} /><span class="name">${isDark() ? t('Light mode') : t('Dark mode')}</span></button>
    </div></nav>`;
}

/** Barra inferior del móvil. */
function Tabbar() {
  const s = useStore(), r = s.route, active = s.jobs.filter((j) => j.status === 'queued' || j.status === 'running').length;
  const go = (v) => { store.set({ jobsOpen: false }); nav({ ...HOME, view: v }); };
  return html`<nav class="tabbar only-narrow">
    <button class=${cls('tab', r.view === 'all' && !s.jobsOpen && !s.menu && 'on')} onClick=${() => go('all')}><${Icon} n="files" /><span>${t('Archive')}</span></button>
    <button class=${cls('tab', r.view === 'inbox' && !s.jobsOpen && !s.menu && 'on')} onClick=${() => go('inbox')}><${Icon} n="inbox" /><span>${t('short|Inbox')}</span>${s.stats.inbox > 0 && html`<i class="badge">${s.stats.inbox}</i>`}</button>
    ${canWrite() ? html`<button class="fab" title=${t('Add documents')} onClick=${() => store.set({ sheet: 'upload' })}><${Icon} n="plus" /></button>` : html`<span />`}
    <button class=${cls('tab', s.jobsOpen && 'on')} onClick=${openJobs}>${active ? html`<span class="spinner" />` : html`<${Icon} n="activity" />`}<span>${t('Activity')}</span>${s.stats.failed > 0 && html`<i class="badge bad">${s.stats.failed}</i>`}</button>
    <button class=${cls('tab', s.menu && 'on')} onClick=${() => store.set({ menu: true })}><${Icon} n="menu" /><span>${t('More')}</span></button></nav>`;
}
function UploadSheet() {
  const close = () => store.set({ sheet: null }), folder = here();
  return html`<div class="veil bottom" onMouseDown=${(e) => e.target === e.currentTarget && close()}><div class="sheet"><div class="grabber" />
    <h3>${folder ? t('Add to “{name}”', { name: folder.name }) : t('Add to Papiro')}</h3>
    <button class="big-opt" onClick=${() => { close(); pickFiles(true); }}><span class="ico"><${Icon} n="camera" /></span><span><b>${t('Take a photo')}</b><small>${t('Photograph a paper document; Papiro extracts the text.')}</small></span></button>
    <button class="big-opt" onClick=${() => { close(); pickFiles(); }}><span class="ico"><${Icon} n="upload" /></span><span><b>${t('Choose files')}</b><small>${t('PDF, photos, Word, Excel, PowerPoint or text.')}</small></span></button>
    <p class="hint">${t('With Papiro installed on your phone, you can also use “Share” from any other app.')}</p></div></div>`;
}

function Jobs() {
  const s = useStore();
  const icon = { queued: html`<span class="faint">◦</span>`, running: html`<span class="spinner" />`, done: html`<span class="ok"><${Icon} n="check" /></span>`, duplicate: html`<span class="warn">⧉</span>`, failed: html`<span class="bad"><${Icon} n="x" /></span>` };
  const label = { queued: t('Queued'), running: t('Extracting text…'), done: t('Done'), duplicate: t('Already in the archive'), failed: t('Failed') };
  const reload = () => api('/jobs').then((jobs) => store.set({ jobs }));
  const close = () => store.set({ jobsOpen: false });
  return html`<div class="veil bottom" onMouseDown=${(e) => e.target === e.currentTarget && close()}><div class="jobs scroll"><div class="grabber" />
    <div class="jobs-head"><b>${t('Activity')}</b><button class="btn icon ghost sm" onClick=${close} title=${t('Close')}><${Icon} n="x" /></button></div>
    ${!s.jobs.length && html`<div class="job muted"><span /><div>${t('Nothing has been processed yet. Upload a file to get started.')}</div></div>`}
    ${s.jobs.slice(0, 40).map((j) => html`<div class="job" key=${j.id}>${icon[j.status]}<div class="grow"><div class="name">${j.filename}</div>
      <small>${label[j.status]}${j.error && j.status === 'failed' ? ` — ${srv(j.error.split('\n')[0])}` : ''}
        ${j.doc_id ? html` · <a href="#" onClick=${(e) => { e.preventDefault(); nav({ doc: j.doc_id }); close(); }}>${t('open')}</a>` : ''}</small>
      ${j.status === 'failed' && canWrite() && html`<div class="row wrap" style="margin-top:7px">
        ${j.can_retry && html`<button class="btn sm" onClick=${() => attempt(api(`/jobs/${j.id}/retry`, { method: 'POST' }), t('Retrying…')).then(reload)}>${t('Retry')}</button>`}
        <button class="btn ghost sm" onClick=${() => attempt(api(`/jobs/${j.id}`, { method: 'DELETE' })).then(reload)}>${t('Dismiss')}</button>
        ${j.can_retry && html`<span class="faint">${t('The file was kept')}</span>`}</div>`}
    </div></div>`)}
  </div></div>`;
}

// ───────────── lista ─────────────
function Snippet({ text }) {
  return text.split(/\u0001(.*?)\u0002/s).map((part, i) => (i % 2 ? html`<mark>${part}</mark>` : part));
}
let pressTimer = 0, pressed = false;
function DocRow({ d, i }) {
  const s = S(), open = s.route.doc === d.id, picked = s.picked.has(d.id);
  const corr = labelOf('correspondent', d.correspondent_id), type = labelOf('doctype', d.doctype_id), folder = d.folder_id !== here()?.id && folderOf(d.folder_id);
  const click = (e) => {
    if (pressed) { pressed = false; return; }
    if (e.metaKey || e.ctrlKey || e.target.closest('.check') || (touch && s.picked.size)) return togglePick(d.id, i, e.shiftKey);
    if (e.shiftKey) return togglePick(d.id, i, true);
    store.set({ cur: i }); nav({ doc: d.id });
  };
  // En pantallas táctiles, mantener pulsado selecciona; después cada toque añade o quita.
  const down = (e) => { if (e.pointerType !== 'mouse' && canWrite()) pressTimer = setTimeout(() => { pressed = true; navigator.vibrate?.(10); togglePick(d.id, i); }, 480); };
  const up = () => clearTimeout(pressTimer);
  const drag = (e) => { const ids = picked ? [...s.picked] : [d.id]; e.dataTransfer.setData(DND, ids.join(',')); e.dataTransfer.effectAllowed = 'move'; };
  return html`<div class=${cls('doc', i === s.cur && 'cur', open && 'open', picked && 'picked')} data-i=${i} onClick=${click} draggable=${!touch && canWrite()} onDragStart=${drag}
      onPointerDown=${down} onPointerUp=${up} onPointerCancel=${up} onPointerLeave=${up} onContextMenu=${(e) => touch && e.preventDefault()}>
    ${d.has_thumb ? html`<img class="thumb" loading="lazy" draggable=${false} src=${`/api/documents/${d.id}/thumb?v=2`} alt="" />` : html`<div class="thumb">${d.original_name.split('.').pop()}</div>`}
    <span class="check"><${Icon} n="check" /></span>
    <div class="body"><div class="title">${d.is_private && html`<${Icon} n="lock" />`}${d.title}${suggestions(d).pend.length > 0 && html`<span class="hint-dot" title=${t('Has suggestions')}><${Icon} n="spark" /></span>`}</div>
      ${d.snippet ? html`<div class="snip"><${Snippet} text=${d.snippet} /></div>`
        : html`<div class="sub">${folder && html`<${Icon} n="folder" />`}${[folder?.name, corr?.name, type?.name, d.pages > 1 && tn(d.pages, '{n} page', '{n} pages')].filter(Boolean).join(' · ') || d.original_name}</div>`}
      ${d.tags.length > 0 && html`<div class="tags">${d.tags.map((t) => labelOf('tag', t)).filter(Boolean).map((l) => html`<${Tag} l=${l} key=${l.id} />`)}</div>`}
    </div><div class="date">${fmtDate(d.created)}${d.expires && dueLabel(d.expires) && html`<span class=${`due ${dueLabel(d.expires)[0]}`}>${dueLabel(d.expires)[1]}</span>`}${d.amount != null && html`<span class="amt">${fmtMoney(d.amount)}</span>`}</div></div>`;
}
let anchor = 0;
function togglePick(id, i, range) {
  const { picked, docs } = S(), next = new Set(picked);
  if (range) for (let k = Math.min(anchor, i); k <= Math.max(anchor, i); k++) next.add(docs[k].id);
  else { next.has(id) ? next.delete(id) : next.add(id); anchor = i; }
  store.set({ picked: next, cur: i });
}
function Crumbs({ f }) {
  const chain = [];
  for (let x = f; x && chain.length < 64; x = folderOf(x.parent_id)) chain.unshift(x);
  return html`<div class="crumbs"><button onClick=${() => nav({ f: null, doc: null })} ...${dropOn(null)}>${t('Archive')}</button>
    ${chain.slice(0, -1).map((x) => html`<${Icon} n="chev" /><button onClick=${() => nav({ f: x.id, doc: null })} ...${dropOn(x.id)}>${x.name}</button>`)}<${Icon} n="chev" /></div>`;
}

function Main() {
  const s = useStore(), r = s.route, input = useRef(), [pop, setPop] = useState(null), folder = here();
  const setQ = useMemo(() => debounce((q) => nav({ q, doc: S().route.doc }, true), 110), []);
  useEffect(() => { if (input.current && input.current.value !== r.q && document.activeElement !== input.current) input.current.value = r.q; }, [r.q]);
  useEffect(() => { !touch && document.querySelector(`.doc[data-i="${s.cur}"]`)?.scrollIntoView({ block: 'nearest' }); }, [s.cur]);
  // Una carpeta borrada (aquí o en otro dispositivo) deja de existir: se vuelve a la raíz.
  // (Se lee el estado vivo: el efecto corre después del render y las carpetas pueden haber llegado entretanto.)
  useEffect(() => { const now = S().route; if (S().meta && now.f && now.view === 'all' && !folderOf(now.f)) nav({ f: null }, true); });
  const more = useRef();
  useEffect(() => { const io = new IntersectionObserver((e) => e[0].isIntersecting && !S().loading && S().docs.length < S().total && loadDocs(true)); more.current && io.observe(more.current); return () => io.disconnect(); });
  const title = folder ? folder.name : { inbox: t('Inbox'), all: t('All documents'), trash: t('Trash'), expiring: t('Due dates') }[r.view];
  const filtered = r.q || r.tags.length || r.c || r.t || r.from || r.to;
  const browsing = r.view === 'all' && !filtered, tiles = browsing ? kids(r.f) : [];
  const corr = labelOf('correspondent', r.c), type = labelOf('doctype', r.t);
  const sorts = [['', r.q ? t('Relevance') : r.view === 'expiring' ? t('Due soonest') : t('Newest')], ['created:asc', t('Oldest')], ['added:desc', t('Recently added')], ['title:asc', t('Title A–Z')]];
  const filter = (key, kind, icon, label, value, onChange, multi) => html`<span class="rel"><button class="chip" onClick=${() => setPop(pop === key ? null : key)}><${Icon} n=${icon} />${label}</button>
    ${pop === key && html`<${Picker} kind=${kind} multi=${multi} value=${value} onChange=${onChange} onClose=${() => setPop(null)} />`}</span>`;
  const empty = filtered ? ['search', t('Nothing matches your search'), t('Try other words or remove a filter.')]
    : folder ? ['folder', t('This folder is empty'), canWrite() ? t('Anything you upload while here is saved in this folder. You can also move documents from their details.') : t('There are no documents here yet.')]
    : r.view === 'inbox' ? ['inbox', t('Inbox zero'), t('New items you haven’t reviewed yet will show up here.')]
    : r.view === 'expiring' ? ['clock', t('Nothing due soon'), t('Documents that are overdue or due in the next 60 days show up here.')]
    : r.view === 'trash' ? ['trash', t('The trash is empty'), t('What you delete stays here until you restore it or delete it for good.')]
    : ['upload', t('Your archive is empty'), touch ? t('Tap + to photograph a document or choose files.') : t('Drag your PDFs, photos or Office documents here to get started.')];
  return html`<main class="main">
    <div class="topbar">
      <div class="row"><div class="search grow"><${Icon} n="search" />
          <input ref=${input} id="q" type="search" enterkeyhint="search" autocomplete="off" placeholder=${folder ? t('Search in {name}…', { name: folder.name }) : t('Search documents…')} defaultValue=${r.q} onInput=${(e) => setQ(e.target.value)}
            onKeyDown=${(e) => { if (e.key === 'Escape' || e.key === 'ArrowDown' || e.key === 'Enter') { e.target.blur(); e.preventDefault(); } }} />
          <span class="hint"><kbd>/</kbd><kbd>⌘K</kbd></span></div>
        <button class="avatar only-narrow" title=${t('{user}: settings', { user: s.user.username })} onClick=${() => store.set({ modal: 'settings:account' })}>${s.user.username[0]}</button></div>
      <div class="filters">
        ${r.tags.map((id) => labelOf('tag', id)).filter(Boolean).map((l) => html`<${Tag} l=${l} onRemove=${() => nav({ tags: r.tags.filter((x) => x !== l.id) })} />`)}
        ${corr && html`<button class="chip on" onClick=${() => nav({ c: null })}><${Icon} n="user" /><span>${corr.name}</span>×</button>`}
        ${type && html`<button class="chip on" onClick=${() => nav({ t: null })}><${Icon} n="type" /><span>${type.name}</span>×</button>`}
        ${filter('tag', 'tag', 'tag', t('Tag'), r.tags, (tags) => nav({ tags }), true)}
        ${filter('c', 'correspondent', 'user', t('Correspondent'), r.c, (c) => nav({ c }))}
        ${filter('t', 'doctype', 'type', t('Type'), r.t, (x) => nav({ t: x }))}
        <label class=${cls('chip', r.from && 'on')}>${t('From')}<input type="date" value=${r.from} onChange=${(e) => nav({ from: e.target.value })} /></label>
        <label class=${cls('chip', r.to && 'on')}>${t('To')}<input type="date" value=${r.to} onChange=${(e) => nav({ to: e.target.value })} /></label>
        ${filtered && html`<button class="btn ghost sm" onClick=${() => { input.current.value = ''; nav({ q: '', tags: [], c: null, t: null, from: '', to: '' }); }}>${t('Clear')}</button>`}
      </div>
    </div>
    <div class=${cls('list scroll', s.picked.size && 'selecting')}>
      <div class="list-head">${folder && html`<${Crumbs} f=${folder} />`}
        <div class="row"><h1 class="grow">${title}</h1>${folder && canWrite() && html`<${MenuButton} items=${folderMenu(folder)} title=${t('Folder options')} right cls="btn icon ghost" />`}</div>
        <div class="row"><span class="sub grow">${s.amountTotal != null ? tn(s.total, '{n} document · total {amount}', '{n} documents · total {amount}', { amount: fmtMoney(s.amountTotal) }) : tn(s.total, '{n} document', '{n} documents')}</span>
          <select title=${t('Sort')} value=${r.sort} onChange=${(e) => nav({ sort: e.target.value })}>${sorts.map(([v, l]) => html`<option value=${v}>${l}</option>`)}</select>
          <button class="btn icon ghost sm" title=${t('Change view (V)')} onClick=${toggleGrid}><${Icon} n=${s.grid ? 'rows' : 'grid'} /></button></div></div>
      ${browsing && (tiles.length > 0 || canWrite()) && html`<div class="tiles">
        ${tiles.map((f) => html`<button class="tile" key=${f.id} onClick=${() => nav({ f: f.id, doc: null })} ...${dropOn(f.id)}><${Icon} n="folder" /><span class="grow"><span class="name">${f.name}</span>
          <small>${tn(f.document_count, '{n} document', '{n} documents')}</small></span></button>`)}
        ${canWrite() && html`<button class="tile new" onClick=${() => newFolder(r.f)}><${Icon} n="plus" /><span class="name">${t('New folder')}</span></button>`}</div>`}
      ${!s.docs.length && !s.loading ? html`<div class="empty"><div><div class="big"><${Icon} n=${empty[0]} /></div><h3>${empty[1]}</h3><div>${empty[2]}</div>
          ${!filtered && r.view === 'all' && canWrite() && html`<button class="btn primary" onClick=${addDocs}><${Icon} n="upload" />${t('Add documents')}</button>`}</div></div>`
        : html`<div class=${s.grid ? 'grid' : ''}>${s.docs.map((d, i) => html`<${DocRow} d=${d} i=${i} key=${d.id} />`)}</div><div ref=${more} style="height:1px" />`}
    </div></main>`;
}
function toggleGrid() { localStorage.grid = S().grid ? '0' : '1'; store.set({ grid: !S().grid }); }

// ───────────── detalle ─────────────
function PageImg({ id, n }) {
  const [bad, setBad] = useState(false);
  if (bad) return html`<div class="page-error"><${Icon} n="help" /><b>${t('Couldn’t show page {n}', { n })}</b>
    <span>${t('The extracted text is still available in the Text tab. If the problem continues, an admin can check it in Settings → Health.')}</span>
    <button class="btn sm" onClick=${() => setBad(false)}>${t('Retry')}</button></div>`;
  return html`<img loading=${n > 1 ? 'lazy' : 'eager'} src=${`/api/documents/${id}/pages/${n}?v=2`} alt=${t('Page {n}', { n })} onError=${() => setBad(true)} />`;
}
function Prop({ k, icon, children, picker }) {
  const [open, setOpen] = useState(false);
  useEffect(() => { const f = (e) => e.detail === k && setOpen(true); addEventListener('open-prop', f); return () => removeEventListener('open-prop', f); }, []);
  return html`<div class="prop"><div class="k"><${Icon} n=${icon} />${k}</div>
    ${picker && canWrite() ? html`<button class="v" onClick=${() => setOpen(true)}>${children}</button>${open && picker(() => setOpen(false))}` : picker ? html`<div class="v">${children}</div>` : children}</div>`;
}
function Detail({ id }) {
  const s = useStore(), [d, setD] = useState(null), [tab, setTab] = useState('doc'), [note, setNote] = useState(''), [busy, setBusy] = useState(false);
  const load = () => api(`/documents/${id}`).then(setD).catch(() => { toast(t('That document no longer exists'), 'bad'); nav({ doc: null }, true); });
  useEffect(() => { setD(null); setTab('doc'); load(); const f = (e) => e.detail.id === id && e.detail.type === 'document.updated' && load(); addEventListener('doc-changed', f); return () => removeEventListener('doc-changed', f); }, [id]);
  if (!d) return html`<section class="detail"><div class="viewer"><div class="viewer-bar"><button class="btn icon ghost" onClick=${() => nav({ doc: null })} title=${t('Back')}><${Icon} n="back" /></button></div><div class="empty"><span class="spinner" /></div></div></section>`;
  const save = async (patch) => { const nd = await attempt(api(`/documents/${id}`, { method: 'PATCH', json: patch })); if (nd) { setD(nd); refresh(); } };
  const pdf = (d.has_archive || d.mime === 'application/pdf') && d.pages > 0, trashed = !!d.deleted_at, ro = !canWrite();
  const mine = isAdmin() || d.owner_id === s.user.id;
  const corr = labelOf('correspondent', d.correspondent_id), type = labelOf('doctype', d.doctype_id), folder = folderOf(d.folder_id);
  const inbox = d.tags.filter((t) => inboxIds().includes(t));
  const words = s.route.q.replace(/"/g, ' ').split(/\s+/).filter((w) => w.length > 1).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  const text = () => words.length ? d.content.split(new RegExp(`(${words.join('|')})`, 'gi')).map((p, i) => (i % 2 ? html`<mark>${p}</mark>` : p)) : d.content;
  const ai = async () => { setBusy(true); const nd = await attempt(api(`/documents/${id}/ai`, { method: 'POST' }), t('Classified with AI')); nd && setD(nd); setBusy(false); refresh(); };
  const addNote = async () => { if (note.trim() && await attempt(api(`/documents/${id}/notes`, { json: { text: note } }))) { setNote(''); load(); } };
  const del = async () => {
    if (trashed && !await ask({ title: t('Delete permanently?'), text: t('“{title}” and its files will be removed from the server. This can’t be undone.', { title: d.title }), ok: t('Delete forever'), danger: true })) return;
    await attempt(api(`/documents/${id}${trashed ? '?permanent=true' : ''}`, { method: 'DELETE' }), trashed ? t('Permanently deleted') : t('Moved to trash')); nav({ doc: null }, true); refresh();
  };
  const sg = suggestions(d);
  const suggest = async (action, fields) => {
    const nd = await attempt(api(`/documents/${id}/suggest`, { json: { action, fields } }));
    if (!nd) return;
    setD(nd); refresh();
    if (action === 'refresh' && !suggestions(nd).pend.length) toast(suggestions(nd).auto.length ? t('What was clear has been applied; no suggestions left') : t('No suggestions: there are no similar filed documents yet'), 'warn');
  };
  const file = `/api/documents/${id}/file`, view = tab === 'meta' ? 'doc' : tab;
  const files = [pdf && ['ext', t('Open the PDF in a new tab'), () => window.open(file, '_blank')], ['down', t('Download the original ({size})', { size: fmtSize(d.size) }), () => { location.href = `${file}?variant=original&download=true`; }],
    d.has_archive && ['down', t('Download the PDF with text (OCR)'), () => { location.href = `${file}?variant=archive&download=true`; }]];
  return html`<section class=${`detail tab-${tab}`}>
    <div class="viewer">
      <div class="viewer-bar"><button class="btn icon ghost" onClick=${() => nav({ doc: null })} title=${t('Back (Esc)')}><${Icon} n="back" /></button>
        <div class=${cls('seg', !pdf && 'only-narrow')}>${pdf && html`<button class=${tab === 'doc' ? 'on' : ''} onClick=${() => setTab('doc')}>${t('Pages')}</button>`}
          <button class=${tab === 'text' || (!pdf && tab === 'doc') ? 'on' : ''} onClick=${() => setTab(pdf ? 'text' : 'doc')}>${t('Text')}</button>
          <button class=${cls('only-narrow', tab === 'meta' && 'on')} onClick=${() => setTab('meta')}>${t('Details')}</button></div>
        <span class="grow" /><${MenuButton} items=${files} icon="down" title=${t('Open or download')} right cls="btn ghost sm"><span class="only-wide">${t('Download')}</span><//></div>
      ${view === 'text' || !pdf ? html`<div class="textview scroll">${d.content ? text() : html`<span class="muted">${t('This document has no extracted text.')}</span>`}</div>`
        : html`<div class="pages scroll" key=${id}>${Array.from({ length: Math.max(d.pages, 1) }, (_, i) => html`<${PageImg} id=${id} n=${i + 1} />`)}</div>`}
    </div>
    <div class="meta scroll">
      <textarea class="title-input" rows="1" key=${id} defaultValue=${d.title} title=${t('Title (tap to edit)')} readonly=${ro} enterkeyhint="done"
        onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); e.target.blur(); } }} onBlur=${(e) => !ro && e.target.value.trim() && e.target.value !== d.title && save({ title: e.target.value.trim() })} />
      ${trashed && html`<div class="row"><span class="chip on">${t('In the trash')}</span>${!ro && html`<button class="btn sm" onClick=${async () => { await attempt(api(`/documents/${id}/restore`, { method: 'POST' }), t('Restored')); load(); refresh(); }}><${Icon} n="restore" />${t('Restore')}</button>`}</div>`}
      ${inbox.length > 0 && !trashed && !ro && html`<button class="btn primary" onClick=${() => save({ remove_tags: inbox })}><${Icon} n="check" />${t('Mark as reviewed')} <kbd class="only-wide" style="background:none;border-color:currentColor;color:inherit;opacity:.7">E</kbd></button>`}
      ${sg.pend.length > 0 && !ro && !trashed && html`<div class="suggest"><h5><span class="row"><${Icon} n="spark" />${t('Suggestions')}</span>
          ${sg.pend.length > 1 && html`<button class="btn primary sm" onClick=${() => suggest('accept')}>${t('Accept all')}</button>`}</h5>
        ${sg.pend.map((x) => html`<div class="sg" key=${x.key}><${Icon} n=${x.icon} /><span class="grow"><small>${tn(x.similar, '{label} · {n} match', '{label} · {n} matches', { label: x.label })}</small><b>${x.name}</b></span>
          <button class="btn icon sm ok" title=${t('Accept')} onClick=${() => suggest('accept', [x.key])}><${Icon} n="check" /></button>
          <button class="btn icon ghost sm" title=${t('Dismiss')} onClick=${() => suggest('dismiss', [x.key])}><${Icon} n="x" /></button></div>`)}
        <small class="faint">${t('Based on similar documents you already filed. Computed on your server.')}</small></div>`}
      <div class="props">
        <${Prop} k=${t('Folder')} icon="folder" picker=${(close) => html`<${FolderPicker} value=${d.folder_id} onChange=${(v) => save({ folder_id: v })} onClose=${close} />`}>${folder ? folder.path.replaceAll('/', ' › ') : html`<span class="faint">${t('No folder')}</span>`}<//>
        <${Prop} k=${t('Date')} icon="cal"><input class="v" type="date" value=${d.created} disabled=${ro} onChange=${(e) => e.target.value && save({ created: e.target.value })} /><//>
        <${Prop} k=${t('Correspondent')} icon="user" picker=${(close) => html`<${Picker} kind="correspondent" value=${d.correspondent_id} onChange=${(v) => save({ correspondent_id: v })} onClose=${close} />`}>${corr ? corr.name : html`<span class="faint">${t('Unassigned')}</span>`}<//>
        <${Prop} k=${t('Type')} icon="type" picker=${(close) => html`<${Picker} kind="doctype" value=${d.doctype_id} onChange=${(v) => save({ doctype_id: v })} onClose=${close} />`}>${type ? type.name : html`<span class="faint">${t('Unassigned')}</span>`}<//>
        <${Prop} k=${t('Tags')} icon="tag" picker=${(close) => html`<${Picker} kind="tag" multi value=${d.tags} onChange=${(tags) => save({ tags })} onClose=${close} />`}>
          ${d.tags.length ? d.tags.map((x) => labelOf('tag', x)).filter(Boolean).map((l) => html`<${Tag} l=${l} key=${l.id} onRemove=${!ro && (() => save({ remove_tags: [l.id] }))} />`) : html`<span class="faint">${ro ? t('None') : t('Add…')}</span>`}<//>
        <${Prop} k=${t('Due date')} icon="clock"><input class="v" type="date" value=${d.expires || ''} disabled=${ro} onChange=${(e) => save({ expires: e.target.value || null })} /><//>
        <${Prop} k=${t('Amount')} icon="cash"><input class="v" type="number" inputmode="decimal" step="0.01" min="0" placeholder="—" key=${`${id}-${d.amount}`} disabled=${ro} defaultValue=${d.amount ?? ''} onBlur=${(e) => String(d.amount ?? '') !== e.target.value && save({ amount: e.target.value ? Number(e.target.value) : null })} /><//>
        <${Prop} k=${t('Archive no.')} icon="key"><input class="v" type="number" inputmode="numeric" min="0" placeholder="—" key=${id} disabled=${ro} defaultValue=${d.asn ?? ''} onBlur=${(e) => String(d.asn ?? '') !== e.target.value && save({ asn: e.target.value ? Number(e.target.value) : null })} /><//>
        ${(d.is_private || (mine && !ro)) && html`<${Prop} k=${t('Private')} icon="lock"><label class="v" title=${t('Only the uploader and admins can see it')}><input type="checkbox" checked=${d.is_private} disabled=${!mine || ro} onChange=${(e) => save({ is_private: e.target.checked })} /> ${d.is_private ? t('Only you and admins') : t('Visible to all users')}</label><//>`}
      </div>
      ${!ro && !trashed && html`<div class="row wrap auto-note">${sg.auto.length > 0 && html`<small class="faint grow">${t('Assigned by similarity only: {list}.', { list: sg.auto.map((x) => t('{label} “{name}”', { label: x.label.toLowerCase(), name: x.name })).join(', ') })}</small>`}
        ${!sg.pend.length && html`<button class="btn ghost sm" onClick=${() => suggest('refresh')}><${Icon} n="spark" />${t('Find suggestions')}</button>`}</div>`}
      ${(d.summary || (s.llm && !ro)) && html`<div class="section"><h5>${t('Summary')} ${s.llm && !ro && html`<button class="btn ghost sm" disabled=${busy} onClick=${ai}>${busy ? html`<span class="spinner" />` : html`<${Icon} n="spark" />`}${d.summary ? t('Regenerate') : t('Classify with AI')}</button>`}</h5>
        ${d.summary && html`<div class="summary">${d.summary}</div>`}</div>`}
      ${Object.keys(d.custom).length > 0 && html`<div class="section"><h5>${t('Fields')}</h5><div class="facts">${Object.entries(d.custom).map(([k, v]) => html`<div>${k}: <b>${String(v)}</b></div>`)}</div></div>`}
      <div class="section"><h5>${t('Notes')}</h5>
        ${d.notes.map((n) => html`<div class="note" key=${n.id}>${n.text}<small>${fmtDate(n.created)}</small>${!ro && html`<button title=${t('Delete note')} onClick=${async () => { await attempt(api(`/notes/${n.id}`, { method: 'DELETE' })); load(); }}><${Icon} n="x" /></button>`}</div>`)}
        ${!ro && html`<textarea class="input" rows="2" placeholder=${t('Write a note…')} value=${note} onInput=${(e) => setNote(e.target.value)} onKeyDown=${(e) => (e.metaKey || e.ctrlKey) && e.key === 'Enter' && addNote()} />`}
        ${ro && !d.notes.length && html`<span class="faint">${t('No notes')}</span>`}
        ${note.trim() && html`<button class="btn sm" style="justify-self:start" onClick=${addNote}>${t('Save note')}</button>`}</div>
      <div class="section"><h5>${t('File')}</h5><div class="facts">
        <div><b>${d.original_name}</b></div><div>${fmtSize(d.size)} · ${tn(d.pages, '{n} page', '{n} pages')} · ${d.mime}</div>
        <div>${t('Added {date}', { date: fmtDate(d.added) })}</div><div class="faint" title=${t('SHA-256 of the original')}>${d.sha256.slice(0, 16)}…</div></div>
        ${!ro && (!trashed || isAdmin()) && html`<button class="btn danger sm" style="justify-self:start" onClick=${del}><${Icon} n="trash" />${trashed ? t('Delete permanently') : t('Move to trash')}</button>`}</div>
    </div></section>`;
}

// ───────────── edición masiva ─────────────
function BulkBar() {
  const s = useStore(), [pop, setPop] = useState(null), ids = s.picked, trash = s.route.view === 'trash';
  const btn = (key, icon, label, kind, apply) => html`<span class="rel"><button class="btn sm" onClick=${() => setPop(pop === key ? null : key)}><${Icon} n=${icon} />${label}</button>
    ${pop === key && (kind === 'folder' ? html`<${FolderPicker} value=${-1} onClose=${() => setPop(null)} onChange=${(v) => moveTo(ids, v)} />`
      : html`<${Picker} kind=${kind} multi=${kind === 'tag'} value=${kind === 'tag' ? [] : null} onClose=${() => setPop(null)} onChange=${(v, l) => { apply(v, l); setPop(null); }} />`)}</span>`;
  const withSug = s.docs.filter((d) => ids.has(d.id) && suggestions(d).pend.length);
  const acceptAll = async () => { await Promise.all(withSug.map((d) => attempt(api(`/documents/${d.id}/suggest`, { json: { action: 'accept' } })))); toast(tn(withSug.length, 'Suggestions accepted on {n} document', 'Suggestions accepted on {n} documents')); store.set({ picked: new Set() }); refresh(); };
  const purge = async () => await ask({ title: tn(ids.size, 'Permanently delete {n} document?', 'Permanently delete {n} documents?'), text: t('They’ll be removed from the server along with their files. This can’t be undone.'), ok: t('Delete forever'), danger: true }) && bulk(ids, { action: 'purge' }, t('Documents permanently deleted'));
  return html`<div class="bulk"><span>${tn(ids.size, '{n} document selected', '{n} documents selected')}</span><span class="sep" />
    ${trash ? html`<button class="btn sm" onClick=${() => bulk(ids, { action: 'restore' }, t('Documents restored'))}><${Icon} n="restore" />${t('Restore')}</button>
      ${isAdmin() && html`<button class="btn sm" onClick=${purge}><${Icon} n="trash" />${t('Delete forever')}</button>`}`
    : html`${withSug.length > 0 && html`<button class="btn sm" onClick=${acceptAll}><${Icon} n="spark" />${t('Accept suggestions')}</button>`}${btn('f', 'folder', t('Move'), 'folder')}
      ${btn('tag', 'tag', t('Add tag'), 'tag', (v, l) => bulk(ids, { add_tags: [l.id] }, t('Tag “{name}” added', { name: l.name })))}
      ${btn('c', 'user', t('Correspondent'), 'correspondent', (v, l) => bulk(ids, { correspondent_id: l.id }, t('Correspondent: {name}', { name: l.name })))}
      ${btn('t', 'type', t('Type'), 'doctype', (v, l) => bulk(ids, { doctype_id: l.id }, t('Type: {name}', { name: l.name })))}
      ${inboxIds().length > 0 && html`<button class="btn sm" onClick=${() => bulk(ids, { remove_tags: inboxIds() }, t('Documents marked as reviewed'))}><${Icon} n="check" />${t('Reviewed')}</button>`}
      <button class="btn sm" onClick=${() => bulk(ids, { action: 'trash' }, t('Documents moved to trash'))}><${Icon} n="trash" />${t('Trash')}</button>`}
    <span class="sep" /><button class="btn icon sm" title=${t('Clear selection (Esc)')} onClick=${() => store.set({ picked: new Set() })}><${Icon} n="x" /></button></div>`;
}

// ───────────── paleta de comandos ─────────────
function Palette() {
  const s = useStore(), [q, setQ] = useState(''), [hi, setHi] = useState(0), [found, setFound] = useState([]);
  const close = () => store.set({ palette: false });
  const search = useMemo(() => debounce(async (q) => setFound(q.trim().length > 1 ? (await api('/documents?limit=6&q=' + encodeURIComponent(q))).items : []), 90), []);
  const actions = [
    [t('Upload documents'), 'upload', () => pickFiles(), 'U'], [t('Go to the inbox'), 'inbox', () => nav({ ...HOME, view: 'inbox' }), 'G I'], [t('Go to all documents'), 'files', () => nav({ ...HOME, view: 'all' }), 'G A'],
    [t('Go to the trash'), 'trash', () => nav({ ...HOME, view: 'trash' }), 'G T'], [t('Go to due dates'), 'clock', () => nav({ ...HOME, view: 'expiring' })], ...(canWrite() ? [[t('New folder'), 'folder', () => newFolder(here()?.id), 'N']] : []), [isDark() ? t('Switch to light mode') : t('Switch to dark mode'), isDark() ? 'sun' : 'moon', () => setTheme(isDark() ? 'light' : 'dark'), 'D'],
    [t('Settings'), 'settings', () => store.set({ modal: 'settings' }), ','], [t('API tokens and MCP'), 'key', () => store.set({ modal: 'settings:tokens' })], ...(isAdmin() ? [[t('Archive health'), 'activity', () => store.set({ modal: 'settings:health' })]] : []), [t('API documentation'), 'code', () => store.set({ modal: 'settings:api' })],
    [t('Keyboard shortcuts'), 'help', () => store.set({ modal: 'help' }), '?'], [t('Language: {name}', { name: LANG === 'es' ? 'English' : 'Español' }), 'globe', () => setLang(LANG === 'es' ? 'en' : 'es')], [t('Sign out'), 'logout', () => api('/auth/logout', { method: 'POST' }).then(() => location.reload())],
    ...s.folders.map((f) => [t('Folder: {name}', { name: f.path.replaceAll('/', ' › ') }), 'folder', () => nav({ ...HOME, view: 'all', f: f.id })]),
    ...s.labels.tag.map((l) => [t('Tag: {name}', { name: l.name }), 'tag', () => nav({ tags: [l.id], doc: null })]),
    ...s.labels.correspondent.map((l) => [t('Correspondent: {name}', { name: l.name }), 'user', () => nav({ c: l.id, doc: null })]),
    ...s.labels.doctype.map((l) => [t('Type: {name}', { name: l.name }), 'type', () => nav({ t: l.id, doc: null })]),
  ].filter(([l]) => l.toLowerCase().includes(q.trim().toLowerCase())).slice(0, q ? 7 : 9);
  const items = [...found.map((d) => [d.title, 'files', () => nav({ doc: d.id }), fmtDate(d.created)]), ...actions];
  const run = (i) => { close(); items[i]?.[2](); };
  const key = (e) => {
    if (e.key === 'ArrowDown') { setHi((hi + 1) % items.length); e.preventDefault(); } else if (e.key === 'ArrowUp') { setHi((hi - 1 + items.length) % items.length); e.preventDefault(); }
    else if (e.key === 'Enter') run(hi); else if (e.key === 'Escape') close();
  };
  return html`<div class="veil" onMouseDown=${(e) => e.target === e.currentTarget && close()}><div class="palette">
    <input autofocus ref=${(el) => el?.focus()} placeholder=${t('Search for a document or type an action…')} value=${q} onInput=${(e) => { setQ(e.target.value); setHi(0); search(e.target.value); }} onKeyDown=${key} />
    <div class="opts scroll">${items.map(([label, icon, , hint], i) => html`<button class=${cls('opt', i === hi && 'hi')} onMouseEnter=${() => setHi(i)} onClick=${() => run(i)}><${Icon} n=${icon} /><span>${label}</span>${hint && html`<small>${hint}</small>`}</button>`)}
      ${!items.length && html`<div class="opt muted">${t('No matches')}</div>`}</div></div></div>`;
}


// ───────────── ajustes ─────────────
const ALGOS = [['none', t('No rule')], ['any', t('Any word')], ['all', t('All words')], ['literal', t('Exact phrase')], ['regex', t('Regular expression')]];
function Labels({ kind, path, title, blurb }) {
  const s = useStore(), [name, setName] = useState('');
  const patch = (l, json) => attempt(api(`/${path}/${l.id}`, { method: 'PATCH', json })).then(loadMeta);
  const add = async (e) => { e.preventDefault(); if (await attempt(api('/' + path, { json: { name } }))) { setName(''); loadMeta(); } };
  return html`<h2>${title}</h2><p>${blurb}</p>
    <form class="row" style="margin-bottom:12px" onSubmit=${add}><input class="input" placeholder=${t('New name…')} value=${name} onInput=${(e) => setName(e.target.value)} required /><button class="btn primary">${t('Add')}</button></form>
    <table class="table"><tr><th></th><th>${t('Name')}</th><th>${t('Assign if the text contains')}</th><th>${t('Rule')}</th>${kind === 'tag' && html`<th title=${t('Added to every new document')}>${t('short|Inbox')}</th>`}<th>Docs</th><th></th></tr>
    ${s.labels[kind].map((l) => html`<tr key=${l.id}>
      <td><input type="color" value=${l.color} onChange=${(e) => patch(l, { color: e.target.value })} /></td>
      <td><input class="input" defaultValue=${l.name} onBlur=${(e) => e.target.value.trim() && e.target.value !== l.name && patch(l, { name: e.target.value.trim() })} /></td>
      <td><input class="input" defaultValue=${l.match} placeholder=${t('e.g. ACME electricity')} onBlur=${(e) => e.target.value !== l.match && patch(l, { match: e.target.value, algo: l.algo === 'none' && e.target.value ? 'any' : l.algo })} /></td>
      <td><select class="input" value=${ALGOS.some(([v]) => v === l.algo) ? l.algo : 'all'} onChange=${(e) => patch(l, { algo: e.target.value })}>${ALGOS.map(([v, l]) => html`<option value=${v}>${l}</option>`)}</select></td>
      ${kind === 'tag' && html`<td style="text-align:center"><input type="checkbox" checked=${l.is_inbox} onChange=${(e) => patch(l, { is_inbox: e.target.checked })} /></td>`}
      <td class="muted">${l.document_count}</td>
      <td><button class="btn icon ghost sm" title=${t('Delete')} onClick=${async () => await ask({ title: t('Delete “{name}”?', { name: l.name }), text: t('The documents are kept; they only lose this assignment.'), ok: t('Delete'), danger: true }) && attempt(api(`/${path}/${l.id}`, { method: 'DELETE' })).then(refresh)}><${Icon} n="trash" /></button></td></tr>`)}</table>`;
}
function Tokens() {
  const [list, setList] = useState([]), [f, setF] = useState({ name: '', read_only: false, expires_days: '' }), [fresh, setFresh] = useState(null);
  const load = () => api('/tokens').then(setList); useEffect(() => { load(); }, []);
  const add = async (e) => { e.preventDefault(); const t = await attempt(api('/tokens', { json: { ...f, expires_days: f.expires_days ? Number(f.expires_days) : null } })); if (t) { setFresh(t.token); setF({ name: '', read_only: false, expires_days: '' }); load(); } };
  const tok = fresh || t('ppr_YOUR_TOKEN');
  return html`<h2>${t('API tokens and MCP')}</h2><p>${t('A token gives scripts, automations or AI assistants access to your archive. It’s shown only once; the server stores only a hash of it.')}</p>
    <form style="display:grid;gap:8px;margin-bottom:12px" onSubmit=${add}><input class="input" placeholder=${t('What is it for? (e.g. n8n, Claude)')} value=${f.name} onInput=${(e) => setF({ ...f, name: e.target.value })} required />
      <div class="row" style="flex-wrap:wrap"><label class="row"><input type="checkbox" checked=${f.read_only} onChange=${(e) => setF({ ...f, read_only: e.target.checked })} />${t('Read-only')}</label>
        <select class="input" style="width:auto" value=${f.expires_days} onChange=${(e) => setF({ ...f, expires_days: e.target.value })}><option value="">${t('Never expires')}</option><option value="30">${t('Expires in {n} days', { n: 30 })}</option><option value="90">${t('Expires in {n} days', { n: 90 })}</option><option value="365">${t('Expires in 1 year')}</option></select>
        <span class="grow" /><button class="btn primary">${t('Create token')}</button></div></form>
    ${fresh && html`<div class="section" style="border:0;padding:0;margin-bottom:14px"><b>${t('Copy your token now; it won’t be shown again:')}</b><div class="code">${fresh}</div></div>`}
    <table class="table"><tr><th>${t('Name')}</th><th>${t('Access')}</th><th>${t('Expires')}</th><th>${t('Last used')}</th><th></th></tr>${list.map((k) => html`<tr><td>${k.name}</td><td class="muted">${k.read_only ? t('Read-only') : t('Full')}</td><td class="muted">${k.expires ? fmtDate(k.expires) : t('Never')}</td><td class="muted">${k.last_used ? fmtDate(k.last_used) : t('Never')}</td>
      <td><button class="btn ghost sm danger" onClick=${() => attempt(api(`/tokens/${k.id}`, { method: 'DELETE' }), t('Token revoked')).then(load)}>${t('Revoke')}</button></td></tr>`)}</table>
    <h2 style="margin-top:26px">${t('Connect an AI (MCP)')}</h2><p>${t('Papiro includes an MCP server so an assistant can search, read, upload and tag documents. With a read-only token, it can only search and read.')}</p>
    <div class="code">claude mcp add --transport http papiro ${location.origin}/mcp --header "Authorization: Bearer ${tok}"</div>
    <p style="margin:14px 0 6px" class="muted">${t('Or through the REST API:')}</p>
    <div class="code">curl -H "Authorization: Bearer ${tok}" "${location.origin}/api/documents?q=${t('invoice')}"</div>`;
}
function Webhooks() {
  const [list, setList] = useState([]), [f, setF] = useState({ url: '', events: '*', secret: '' });
  const load = () => api('/webhooks').then(setList).catch(() => {}); useEffect(() => { load(); }, []);
  const add = async (e) => { e.preventDefault(); if (await attempt(api('/webhooks', { json: f }), t('Webhook created'))) { setF({ url: '', events: '*', secret: '' }); load(); } };
  return html`<h2>Webhooks</h2><p>${t('Papiro sends an HTTP POST when a document is created, changed or deleted. With a secret, each delivery is signed (X-Papiro-Signature header, HMAC-SHA256).')}</p>
    <form style="display:grid;gap:8px;margin-bottom:14px" onSubmit=${add}><input class="input" type="url" placeholder=${t('https://your-server/webhook')} value=${f.url} onInput=${(e) => setF({ ...f, url: e.target.value })} required />
      <div class="row"><select class="input" value=${f.events} onChange=${(e) => setF({ ...f, events: e.target.value })}><option value="*">${t('All events')}</option><option value="document.created">${t('New documents only')}</option><option value="document.created,document.updated">${t('New and changed')}</option></select>
        <input class="input" placeholder=${t('Signing secret (optional)')} value=${f.secret} onInput=${(e) => setF({ ...f, secret: e.target.value })} /><button class="btn primary">${t('Add')}</button></div></form>
    <table class="table"><tr><th>URL</th><th>${t('Events')}</th><th>${t('Last delivery')}</th><th></th></tr>${list.map((w) => html`<tr><td style="overflow-wrap:anywhere">${w.url}${w.signed && html` <span class="chip">${t('signed')}</span>`}</td><td class="muted">${w.events === '*' ? t('all') : w.events}</td>
      <td class="muted">${w.last_status ? `${w.last_status} · ${fmtDate(w.last_at)}` : '—'}</td>
      <td class="row"><button class="btn ghost sm" onClick=${() => attempt(api(`/webhooks/${w.id}/test`, { method: 'POST' })).then((r) => { r && toast(t('Response: {status}', { status: r.status }), r.status.startsWith('2') ? 'ok' : 'bad'); load(); })}>${t('Test')}</button>
        <button class="btn icon ghost sm" onClick=${() => attempt(api(`/webhooks/${w.id}`, { method: 'DELETE' })).then(load)}><${Icon} n="trash" /></button></td></tr>`)}</table>`;
}
const ROLES = [['admin', t('Admin'), t('everything, including permanent deletion and managing users')], ['editor', 'Editor', t('uploads and edits; can’t delete permanently')], ['viewer', t('Viewer'), t('only searches, reads and downloads')]];
function Account() {
  const s = useStore(), [users, setUsers] = useState([]), [pw, setPw] = useState({ current: '', new: '' }), [nu, setNu] = useState({ username: '', password: '', role: 'editor' });
  const load = () => isAdmin() && api('/users').then(setUsers); useEffect(() => { load(); }, []);
  const change = async (e) => { e.preventDefault(); await attempt(api('/auth/password', { json: pw }).then(() => setPw({ current: '', new: '' })), t('Password updated')); };
  const add = async (e) => { e.preventDefault(); if (await attempt(api('/users', { json: nu }), t('User created'))) { setNu({ username: '', password: '', role: 'editor' }); load(); } };
  const reset = async (u) => { const p = await ask({ title: t('New password for {user}', { user: u.username }), text: t('At least 8 characters. Their open sessions will be signed out.'), input: true, type: 'password', minlength: 8, ok: t('Reset') }); if (p) await attempt(api(`/users/${u.id}`, { method: 'PATCH', json: { password: p } }), t('Password reset')); };
  return html`<h2>${t('Account')}</h2><p>${t('Signed in as {user} ({role}).', { user: html`<b>${s.user.username}</b>`, role: ROLES.find((r) => r[0] === s.user.role)?.[1] })} <a href="#" onClick=${(e) => { e.preventDefault(); api('/auth/logout', { method: 'POST' }).then(() => location.reload()); }}>${t('Sign out')}</a></p>
    <form class="row" style="flex-wrap:wrap" onSubmit=${change}><input class="input" style="flex:1;min-width:150px" type="password" placeholder=${t('Current password')} value=${pw.current} onInput=${(e) => setPw({ ...pw, current: e.target.value })} required />
      <input class="input" style="flex:1;min-width:150px" type="password" placeholder=${t('New (min. 8)')} minlength="8" value=${pw.new} onInput=${(e) => setPw({ ...pw, new: e.target.value })} required /><button class="btn">${t('Change')}</button></form>
    <h2 style="margin-top:26px">${t('Language')}</h2><select class="input" style="width:auto" value=${LANG} onChange=${(e) => setLang(e.target.value)}><option value="en">English</option><option value="es">Español</option></select>
    ${isAdmin() && html`<h2 style="margin-top:26px">${t('Users')}</h2><p>${ROLES.map(([, n, d]) => html`<b>${n}</b>: ${d}. `)}${t('A document marked as private can only be seen by its uploader and admins.')}</p>
      <form class="row" style="margin-bottom:12px;flex-wrap:wrap" onSubmit=${add}><input class="input" style="flex:1;min-width:120px" placeholder=${t('Username')} value=${nu.username} onInput=${(e) => setNu({ ...nu, username: e.target.value })} required />
        <input class="input" style="flex:1;min-width:120px" type="password" placeholder=${t('Password')} minlength="8" value=${nu.password} onInput=${(e) => setNu({ ...nu, password: e.target.value })} required />
        <select class="input" style="width:auto" value=${nu.role} onChange=${(e) => setNu({ ...nu, role: e.target.value })}>${ROLES.map(([v, n]) => html`<option value=${v}>${n}</option>`)}</select><button class="btn primary">${t('Add')}</button></form>
      <table class="table">${users.map((u) => html`<tr><td>${u.username}</td>
        <td><select class="input" style="width:auto" value=${u.role} disabled=${u.id === s.user.id} onChange=${(e) => attempt(api(`/users/${u.id}`, { method: 'PATCH', json: { role: e.target.value } }), t('Role updated')).then(load)}>${ROLES.map(([v, n]) => html`<option value=${v}>${n}</option>`)}</select></td>
        <td style="text-align:right;white-space:nowrap"><button class="btn ghost sm" onClick=${() => reset(u)}>${t('Reset password')}</button>
          ${u.id !== s.user.id && html`<button class="btn ghost sm danger" onClick=${async () => await ask({ title: t('Delete the user {user}?', { user: u.username }), text: t('Their documents are kept.'), ok: t('Delete user'), danger: true }) && attempt(api(`/users/${u.id}`, { method: 'DELETE' })).then(load)}>${t('Delete')}</button>`}</td></tr>`)}</table>`}`;
}
function Health() {
  const [st, setSt] = useState(null), [busy, setBusy] = useState(false);
  const load = () => api('/status').then(setSt); useEffect(() => { load(); }, []);
  if (!st) return html`<span class="spinner" />`;
  const v = st.last_verify, b = st.last_backup, ok = v && !v.problems.length && v.database === 'ok' && v.search_index_ok;
  const verify = async () => { setBusy(true); await attempt(api('/verify', { method: 'POST' }), t('Check finished')); setBusy(false); load(); };
  return html`<h2>${t('Archive health')}</h2><p>${t(st.ocr_rotate ? 'Papiro {version} · schema {schema} · OCR {langs} with rotated-page correction.' : 'Papiro {version} · schema {schema} · OCR {langs}.', { version: st.version, schema: st.schema, langs: st.ocr_langs })}</p>
    <div class="health">
      <div class=${cls('card', v ? (ok ? 'good' : 'bad') : '')}><b>${t('Integrity')}</b>
        ${v ? html`<span>${ok ? tn(v.documents, 'The only document has its file and its SHA-256 fingerprint matches.', 'All {n} documents have their file and their SHA-256 fingerprints match.') : tn(v.problems.length, '{n} document has problems.', '{n} documents have problems.')}</span>
          <small>${t('Last check: {date} ({secs} s) · database: {db} · search index: {index}', { date: fmtDate(v.at), secs: v.seconds, db: v.database, index: v.search_index_ok ? 'ok' : t('damaged') })}</small>`
          : html`<span>${t('Not checked yet. It runs by itself every day; you can also start it now.')}</span>`}
        <button class="btn sm" style="justify-self:start" disabled=${busy} onClick=${verify}>${busy ? html`<span class="spinner" />` : null}${t('Check now')}</button></div>
      <div class=${cls('card', st.backup_every_hours ? (b?.ok === false ? 'bad' : b ? 'good' : '') : 'warn')}><b>${t('Backup')}</b>
        ${st.backup_every_hours ? html`<span>${t('Automatic every {n} h.', { n: st.backup_every_hours })}</span>` : html`<span>${t('Automatic backup is {off}. Set {dir} to another volume or disk, or run {cmd} from your own scheduler.', { off: html`<b>${t('turned off')}</b>`, dir: html`<code>PAPIRO_BACKUP_DIR</code>`, cmd: html`<code>papiro backup</code>` })}</span>`}
        ${b && html`<small>${b.ok === false ? t('Last attempt failed: {error}', { error: srv(b.error) }) : tn(b.documents, 'Last backup: {date} · {n} document · {size} in {dest}', 'Last backup: {date} · {n} documents · {size} in {dest}', { date: fmtDate(b.at), size: fmtSize(b.bytes), dest: b.dest }) + (b.history ? ' · ' + tn(b.history, '{n} daily copy of the database', '{n} daily copies of the database') : '')}</small>`}</div>
      <div class=${cls('card', st.failed_jobs ? 'warn' : 'good')}><b>${t('Ingestion')}</b><span>${t(st.failed_jobs ? '{queued} queued · {failed} failed (their files were kept: review them in Activity) · {hooks} pending webhook deliveries.' : '{queued} queued · {failed} failed · {hooks} pending webhook deliveries.', { queued: st.queued, failed: st.failed_jobs, hooks: st.pending_webhooks })}</span></div>
    </div>
    ${v?.problems.length > 0 && html`<h2 style="margin-top:22px">${t('Documents with problems')}</h2><p>${t('If you still have the file, upload it again: Papiro recognizes it’s the same one and replaces the damaged copy.')}</p>
      <table class="table">${v.problems.map((p) => html`<tr><td><a href="#" onClick=${(e) => { e.preventDefault(); store.set({ modal: null }); nav({ doc: p.document_id }); }}>${p.title}</a></td><td class="err">${srv(p.problem)}</td></tr>`)}</table>`}`;
}
function Audit() {
  const [rows, setRows] = useState(null); useEffect(() => { api('/audit?limit=200').then(setRows); }, []);
  if (!rows) return html`<span class="spinner" />`;
  return html`<h2>${t('Audit log')}</h2><p>${t('Who did what and from where: sign-ins, uploads, edits, deletions and admin changes.')}</p>
    <table class="table"><tr><th>${t('When')}</th><th>${t('Who')}</th><th>${t('What')}</th><th>${t('Target')}</th><th>IP</th></tr>
    ${rows.map((r) => html`<tr><td class="muted" style="white-space:nowrap">${new Date(r.at.replace(' ', 'T') + 'Z').toLocaleString(LOCALE, { dateStyle: 'short', timeStyle: 'short' })}</td><td>${r.username}</td><td>${srv(r.action)}</td>
      <td class="muted" style="overflow-wrap:anywhere">${r.target}${r.detail ? ` — ${r.detail}` : ''}</td><td class="faint">${r.ip}</td></tr>`)}</table>`;
}
function ApiDocs() {
  const [spec, setSpec] = useState(null); useEffect(() => { api('/openapi.json').then(setSpec); }, []);
  if (!spec) return html`<span class="spinner" />`;
  const groups = {};
  for (const [path, ops] of Object.entries(spec.paths)) for (const [m, op] of Object.entries(ops)) (groups[op.tags?.[0] || t('other')] ||= []).push({ path, m, op });
  const schema = (c) => { const sc = c && Object.values(c)[0]?.schema; return sc ? JSON.stringify(deref(sc), null, 1) : null; };
  const deref = (sc, depth = 0) => { if (!sc || typeof sc !== 'object' || depth > 4) return sc; if (sc.$ref) return deref(spec.components.schemas[sc.$ref.split('/').pop()], depth + 1);
    return Array.isArray(sc) ? sc.map((x) => deref(x, depth)) : Object.fromEntries(Object.entries(sc).map(([k, v]) => [k, deref(v, depth + 1)])); };
  return html`<div class="apidoc"><h2>${spec.info.title} <small class="muted">v${spec.info.version}</small></h2><p style="white-space:pre-wrap">${spec.info.description}</p>
    <p>${t('OpenAPI 3.1 spec:')} <a href="/api/openapi.json" target="_blank">/api/openapi.json</a></p>
    ${Object.entries(groups).map(([tag, ops]) => html`<h3>${tag}</h3>${ops.map(({ path, m, op }) => html`<details class="op"><summary><span class=${`method ${m}`}>${m}</span><span class="path">${path}</span><span class="muted grow">${op.summary}</span></summary>
      <div class="opbody">${op.description && html`<p>${op.description}</p>`}
        ${op.parameters?.length > 0 && html`<table class="table">${op.parameters.map((p) => html`<tr><td><code>${p.name}</code></td><td class="muted">${p.in}${p.required ? ` · ${t('required')}` : ''}</td><td>${p.description || ''}</td></tr>`)}</table>`}
        ${op.requestBody && html`<p><b>${t('Body')}</b> (${Object.keys(op.requestBody.content)[0]})</p><div class="code">${schema(op.requestBody.content)}</div>`}
        <p><b>${t('Responses')}</b></p>${Object.entries(op.responses).map(([code, r]) => html`<details><summary><span class="method">${code}</span><span class="muted">${r.description || (r.content ? Object.keys(r.content)[0] : '')}</span></summary>${schema(r.content) && html`<div class="code">${schema(r.content)}</div>`}</details>`)}
        <p><b>${t('Example')}</b></p><div class="code">curl -X ${m.toUpperCase()} -H "Authorization: Bearer $PAPIRO_TOKEN" "${location.origin}${path}"</div></div></details>`)}`)}</div>`;
}
function Settings({ tab }) {
  const [tb, setT] = useState(tab || 'tags'), s = useStore();
  useEffect(() => { document.querySelector('.modal aside .nav.on')?.scrollIntoView({ inline: 'center', block: 'nearest' }); }, [tb]);
  const tabs = [['tags', 'tag', t('Tags')], ['correspondents', 'user', t('Correspondents')], ['doctypes', 'type', t('Types')], ['tokens', 'key', t('Tokens & MCP')], ...(isAdmin() ? [['webhooks', 'hook', 'Webhooks'], ['health', 'activity', t('Health')], ['audit', 'rows', t('Audit log')]] : []), ['api', 'code', 'API'], ['account', 'settings', t('Account')]];
  return html`<div class="veil full" onMouseDown=${(e) => e.target === e.currentTarget && store.set({ modal: null })}><div class="modal">
    <aside><button class="btn icon ghost close" title=${t('Close')} onClick=${() => store.set({ modal: null })}><${Icon} n="back" /></button><h3>${t('Settings')}</h3>${tabs.map(([k, icon, label]) => html`<button class=${cls('nav', tb === k && 'on')} onClick=${() => setT(k)}><${Icon} n=${icon} /><span class="name">${label}</span></button>`)}</aside>
    <section class="scroll">${tb === 'tags' ? html`<${Labels} kind="tag" path="tags" title=${t('Tags')} blurb=${t('Tags marked as “Inbox” are added automatically to every new document; remove them once you’ve reviewed it.')} />`
      : tb === 'correspondents' ? html`<${Labels} kind="correspondent" path="correspondents" title=${t('Correspondents')} blurb=${t('Who issues or sends the document: companies, banks, agencies, people.')} />`
      : tb === 'doctypes' ? html`<${Labels} kind="doctype" path="doctypes" title=${t('Document types')} blurb=${t('Invoice, contract, receipt, policy… Rules assign the type on ingestion.')} />`
      : tb === 'tokens' ? html`<${Tokens} />` : tb === 'webhooks' ? html`<${Webhooks} />` : tb === 'health' ? html`<${Health} />` : tb === 'audit' ? html`<${Audit} />` : tb === 'api' ? html`<${ApiDocs} />` : html`<${Account} />`}</section></div></div>`;
}
function Help() {
  const keys = [['/', t('Search')], ['⌘K · Ctrl K', t('Command palette')], ['J · ↓', t('Next document')], ['K · ↑', t('Previous document')], ['↵ · O', t('Open preview')], ['Esc', t('Close / clear selection')],
    ['X', t('Select')], [t('Shift + click'), t('Select range')], ['⌘A · Ctrl A', t('Select all')], ['E', t('Mark as reviewed')], ['T', t('Add tag')], ['C', t('Correspondent')], ['M', t('Move to a folder')], ['N', t('New folder')],
    [t('#  ·  Del'), t('Move to trash')], ['U', t('Upload documents')], ['V', t('List / grid')], ['D', t('Light / dark mode')], [t('G then I · A · T'), t('Inbox · All · Trash')], [',', t('Settings')], ['?', t('This help')]];
  return html`<div class="veil center" onMouseDown=${(e) => e.target === e.currentTarget && store.set({ modal: null })}><div class="modal narrow scroll" style="width:min(640px,100%)"><div class="row"><h2 class="grow">${t('Keyboard shortcuts')}</h2><button class="btn icon ghost sm" title=${t('Close')} onClick=${() => store.set({ modal: null })}><${Icon} n="x" /></button></div><p class="muted">${t('You can do everything without leaving the keyboard.')}</p>
    <div class="keys">${keys.map(([k, l]) => html`<div><span>${l}</span><kbd>${k}</kbd></div>`)}</div></div></div>`;
}


// ───────────── raíz y teclado ─────────────
let gPending = 0;
function onKey(e) {
  const s = S(), el = e.target, typing = /INPUT|TEXTAREA|SELECT/.test(el.tagName) || el.isContentEditable, mod = e.metaKey || e.ctrlKey;
  if (s.dialog) { if (e.key === 'Escape') { s.dialog.resolve(null); store.set({ dialog: null }); } return; }
  if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); return store.set({ palette: !s.palette }); }
  if (e.key === 'Escape') {
    if (s.palette || s.modal || s.jobsOpen || s.sheet || s.menu) return store.set({ palette: false, modal: null, jobsOpen: false, sheet: null, menu: false });
    if (typing) return el.blur();
    if (s.picked.size) return store.set({ picked: new Set() });
    if (s.route.doc) return nav({ doc: null });
    return;
  }
  if (typing || s.palette || s.modal || !s.user || e.altKey) return;
  const cur = s.docs[s.cur], targets = s.picked.size ? s.picked : s.route.doc ? new Set([s.route.doc]) : cur ? new Set([cur.id]) : new Set();
  const move = (d) => { const i = Math.max(0, Math.min(s.docs.length - 1, s.cur + d)); store.set({ cur: i }); if (s.route.doc && s.docs[i]) nav({ doc: s.docs[i].id }, true); };
  if (mod && e.key.toLowerCase() === 'a') { e.preventDefault(); return store.set({ picked: new Set(s.docs.map((d) => d.id)) }); }
  if (mod) return;
  if (gPending && Date.now() - gPending < 900) { gPending = 0; const v = { i: 'inbox', a: 'all', t: 'trash' }[e.key.toLowerCase()]; if (v) { e.preventDefault(); return nav({ ...HOME, view: v }); } }
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key, prop = (name) => () => s.route.doc && dispatchEvent(new CustomEvent('open-prop', { detail: name }));
  const map = {
    '/': () => document.getElementById('q')?.focus(), j: () => move(1), ArrowDown: () => move(1), k: () => move(-1), ArrowUp: () => move(-1),
    Enter: () => cur && nav({ doc: cur.id }), o: () => cur && nav({ doc: cur.id }), x: () => cur && togglePick(cur.id, s.cur), g: () => { gPending = Date.now(); },
    u: () => pickFiles(), v: toggleGrid, d: () => setTheme(isDark() ? 'light' : 'dark'), ',': () => store.set({ modal: 'settings' }), '?': () => store.set({ modal: 'help' }),
    n: () => canWrite() && s.route.view === 'all' && newFolder(s.route.f),
    e: () => targets.size && inboxIds().length && bulk(targets, { remove_tags: inboxIds() }, t('Marked as reviewed')),
    '#': () => targets.size && s.route.view !== 'trash' && bulk(targets, { action: 'trash' }, t('Moved to trash')), Delete: () => map['#'](),
    t: prop(t('Tags')), c: prop(t('Correspondent')), m: prop(t('Folder')),
  };
  if (map[k]) { e.preventDefault(); map[k](); }
}
function App() {
  const s = useStore(), [drag, setDrag] = useState(false);
  useEffect(() => {
    let depth = 0;
    const has = (e) => [...(e.dataTransfer?.types || [])].includes('Files');
    const enter = (e) => { if (has(e)) { depth++; setDrag(true); e.preventDefault(); } }, leave = () => { if (--depth <= 0) { depth = 0; setDrag(false); } };
    const over = (e) => has(e) && e.preventDefault(), drop = (e) => { if (has(e)) { e.preventDefault(); depth = 0; setDrag(false); S().user && upload(e.dataTransfer.files); } };
    addEventListener('keydown', onKey); addEventListener('dragenter', enter); addEventListener('dragleave', leave); addEventListener('dragover', over); addEventListener('drop', drop);
    addEventListener('paste', (e) => S().user && !/INPUT|TEXTAREA/.test(e.target.tagName) && e.clipboardData.files.length && upload(e.clipboardData.files));
    boot();
  }, []);
  if (!s.ready) return html`<div class="auth"><span class="spinner" /></div>`;
  if (s.down) return html`<div class="auth"><div class="empty"><div><div class="big"><${Icon} n="activity" /></div><h3>${t('No connection to Papiro')}</h3>
    <div>${t('Your documents are on your server: you need a connection to see them. It retries by itself as soon as it’s back.')}</div>
    <button class="btn primary" onClick=${() => { store.set({ ready: false }); boot(); }}>${t('Retry')}</button></div></div></div>`;
  if (s.setup || !s.user) return html`<${Auth} setup=${s.setup} /><${Toasts} />`;
  const [modal, tab] = (s.modal || '').split(':'), folder = here();
  return html`${!s.online && html`<div class="offline">${t('Offline: you’re seeing what was last loaded')}</div>`}
    <div class=${cls('shell', s.route.doc && 'has-doc')}>
    <${Sidebar} /><${Main} />${s.route.doc && html`<${Detail} id=${s.route.doc} key=${s.route.doc} />`}</div>
    ${s.menu && html`<div class="scrim only-narrow" onClick=${() => store.set({ menu: false })} />`}
    ${!s.route.doc && html`<${Tabbar} />`}
    ${s.picked.size > 0 && canWrite() && html`<${BulkBar} />`}${s.jobsOpen && html`<${Jobs} />`}${s.sheet === 'upload' && html`<${UploadSheet} />`}${s.palette && html`<${Palette} />`}
    ${modal === 'settings' && html`<${Settings} tab=${tab} />`}${modal === 'help' && html`<${Help} />`}${s.dialog && html`<${Dialog} d=${s.dialog} key=${s.dialog.id} />`}
    ${drag && html`<div class="drop"><div>${folder ? t('Drop to add to “{name}”', { name: folder.name }) : t('Drop to add to Papiro')}<small>${t('PDF, images, Word, Excel, PowerPoint, text')}</small></div></div>`}<${Toasts} />`;
}
function Toasts() {
  const s = useStore(), icon = { ok: 'check', bad: 'x', warn: 'help' };
  return html`<div class="toasts">${s.toasts.map((t) => html`<div class=${cls('toast', t.kind === 'bad' && 'bad')} key=${t.id}><span class=${t.kind}><${Icon} n=${icon[t.kind]} /></span><span>${t.text}</span></div>`)}</div>`;
}
render(html`<${App} />`, document.getElementById('app'));
