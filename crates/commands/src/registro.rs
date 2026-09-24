/// Un comando disponible en la paleta de comandos (`Ctrl+Shift+P`,
/// PLAN.md §4). `id` es el mismo identificador que usa `tcode-keymap`
/// (`"archivo.guardar"`); `descripcion` es lo que se busca y se muestra,
/// con el formato `Categoría: Acción` (igual que VSCode) para que sea
/// fácil de escanear visualmente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comando {
    pub id: &'static str,
    pub descripcion: &'static str,
}

/// Comandos que tiene sentido invocar *por nombre* desde la paleta — no
/// todo lo que existe en el keymap encaja aquí: mover el cursor letra a
/// letra o insertar un salto de línea no son "acciones" que alguien busca
/// por nombre, son atajos de uso continuo. Cuando se añade un comando
/// nuevo con sentido de paleta (a `ejecutar_comando` en `app`), hay que
/// añadirlo aquí también — no hay (todavía) un registro dinámico.
pub fn comandos_disponibles() -> &'static [Comando] {
    &[
        Comando { id: "archivo.guardar", descripcion: "Archivo: Guardar" },
        Comando { id: "archivo.guardar_como", descripcion: "Archivo: Guardar como..." },
        Comando { id: "app.salir", descripcion: "Aplicación: Salir del editor" },
        Comando { id: "editor.deshacer", descripcion: "Editor: Deshacer" },
        Comando { id: "editor.rehacer", descripcion: "Editor: Rehacer" },
        Comando { id: "panel.alternar_lateral", descripcion: "Ver: Alternar explorador de archivos" },
        Comando { id: "pestana.siguiente", descripcion: "Pestañas: Siguiente pestaña" },
        Comando { id: "pestana.anterior", descripcion: "Pestañas: Pestaña anterior" },
        Comando { id: "pestana.cerrar", descripcion: "Pestañas: Cerrar pestaña" },
        Comando { id: "pestana.ir_a_1", descripcion: "Pestañas: Ir a la pestaña 1" },
        Comando { id: "pestana.ir_a_2", descripcion: "Pestañas: Ir a la pestaña 2" },
        Comando { id: "pestana.ir_a_3", descripcion: "Pestañas: Ir a la pestaña 3" },
        Comando { id: "pestana.ir_a_4", descripcion: "Pestañas: Ir a la pestaña 4" },
        Comando { id: "pestana.ir_a_5", descripcion: "Pestañas: Ir a la pestaña 5" },
        Comando { id: "pestana.ir_a_6", descripcion: "Pestañas: Ir a la pestaña 6" },
        Comando { id: "pestana.ir_a_7", descripcion: "Pestañas: Ir a la pestaña 7" },
        Comando { id: "pestana.ir_a_8", descripcion: "Pestañas: Ir a la pestaña 8" },
        Comando { id: "pestana.ir_a_9", descripcion: "Pestañas: Ir a la pestaña 9" },
        Comando { id: "vista.modo_zen", descripcion: "Ver: Alternar modo zen (solo el código)" },
        Comando { id: "vista.pantalla_completa", descripcion: "Ver: Maximizar/restaurar el panel activo" },
        Comando { id: "explorador.saltar", descripcion: "Ver: Saltar a un archivo (etiquetas de una tecla)" },
        Comando { id: "explorador.nuevo_archivo", descripcion: "Explorador: Nuevo archivo" },
        Comando { id: "explorador.nueva_carpeta", descripcion: "Explorador: Nueva carpeta" },
        Comando { id: "explorador.renombrar", descripcion: "Explorador: Renombrar selección" },
        Comando { id: "config.recargar", descripcion: "Configuración: Recargar" },
        Comando {
            id: "proyecto.confiar",
            descripcion: "Proyecto: Confiar en este proyecto (aplicar sus comandos LSP y formateadores)",
        },
        Comando {
            id: "proyecto.dejar_de_confiar",
            descripcion: "Proyecto: Revocar confianza (volver a ignorar sus comandos)",
        },
        Comando { id: "tema.seleccionar", descripcion: "Tema: Seleccionar (con preview en vivo)" },
        Comando { id: "tema.editor_visual", descripcion: "Tema: Editor visual (colores por código hex)" },
        Comando { id: "admin.abrir_panel", descripcion: "Panel de administración: Abrir" },
        Comando { id: "lsp.ver_logs", descripcion: "LSP: Ver logs de la sesión activa" },
        Comando { id: "markdown.alternar_preview", descripcion: "Markdown: Alternar preview" },
        Comando { id: "markdown.preview_solo", descripcion: "Markdown: Ver solo preview" },
        Comando { id: "csv.alternar_vista_tabla", descripcion: "CSV: Alternar vista de tabla" },
        Comando { id: "csv.ordenar", descripcion: "CSV: Ordenar por la columna actual (alterna asc/desc)" },
        Comando { id: "csv.filtrar", descripcion: "CSV: Filtrar por la columna actual" },
        Comando { id: "csv.quitar_filtro", descripcion: "CSV: Quitar filtro" },
        Comando { id: "csv.insertar_fila_debajo", descripcion: "CSV: Insertar fila debajo" },
        Comando { id: "csv.insertar_fila_arriba", descripcion: "CSV: Insertar fila arriba" },
        Comando { id: "csv.insertar_columna_derecha", descripcion: "CSV: Insertar columna a la derecha" },
        Comando { id: "csv.insertar_columna_izquierda", descripcion: "CSV: Insertar columna a la izquierda" },
        Comando { id: "csv.eliminar_fila", descripcion: "CSV: Eliminar fila" },
        Comando { id: "csv.eliminar_columna", descripcion: "CSV: Eliminar columna" },
        Comando { id: "csv.ensanchar_columna", descripcion: "CSV: Ensanchar columna" },
        Comando { id: "csv.angostar_columna", descripcion: "CSV: Angostar columna" },
        Comando { id: "csv.restablecer_ancho", descripcion: "CSV: Restablecer ancho automático de columna" },
        Comando {
            id: "cursor.seleccionar_siguiente_ocurrencia",
            descripcion: "Selección: Agregar la siguiente ocurrencia",
        },
        Comando { id: "cursor.seleccionar_todas_ocurrencias", descripcion: "Selección: Seleccionar todas las ocurrencias" },
        Comando { id: "plegar.actual", descripcion: "Plegado: Plegar el bloque del cursor" },
        Comando { id: "plegar.desplegar", descripcion: "Plegado: Desplegar el bloque del cursor" },
        Comando { id: "plegar.todo", descripcion: "Plegado: Plegar todo" },
        Comando { id: "simbolos.ir_a", descripcion: "Ir: Símbolo del archivo (funciones, clases...)" },
        Comando { id: "plegar.desplegar_todo", descripcion: "Plegado: Desplegar todo" },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_comandos_disponibles_tienen_id_y_descripcion_unicos() {
        let comandos = comandos_disponibles();
        assert!(!comandos.is_empty());

        let mut ids: Vec<&str> = comandos.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), comandos.len(), "hay ids de comando repetidos");

        assert!(comandos.iter().all(|c| !c.id.is_empty() && !c.descripcion.is_empty()));
    }
}
