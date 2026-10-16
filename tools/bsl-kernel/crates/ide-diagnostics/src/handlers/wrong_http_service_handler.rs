use crate::define_metadata;
use crate::metadata::*;
use crate::{Diagnostic, DiagnosticCode};
use hir::{ModItem, ModuleMetadata, Name};
use ide_db::TextRange;

pub const METADATA: DiagnosticMetadata = define_metadata! {
    diagnostic_type: DiagnosticType::Error,
    severity: DiagnosticSeverityLevel::Critical,
    scope: DiagnosticScope::Bsl,
    modules: &[bsl_metadata::ModuleType::HTTPServiceModule],
    minutes_to_fix: 10,
    activated_by_default: true,
    compatibility_mode: DiagnosticCompatibilityMode::Undefined,
    tags: &[MetadataTag::Suspicious, MetadataTag::Error],
    can_locate_on_project: false,
    extra_min_for_complexity: 0.0,
    lsp_severity_override: "",
};

pub fn from_metadata(
    metadata: &ModuleMetadata,
    ctx: &crate::DiagnosticsContext,
) -> Vec<Diagnostic> {
    let code = DiagnosticCode::WrongHttpServiceHandler;

    if ctx.is_disabled_with_metadata(code) {
        return Vec::new();
    }

    if metadata.module_type != bsl_metadata::ModuleType::HTTPServiceModule {
        return Vec::new();
    }

    let Some(ref http_service) = metadata.http_service else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let symbol_tree = ctx.symbol_tree();
    let item_tree = ctx.item_tree();
    let header_range = ctx.file_header_range(1);

    for (url_template, method) in http_service.all_methods() {
        let handler_name = method.handler();
        let service_path = format!(
            "HTTPService.{}.URLTemplate.{}.Method.{}",
            http_service.name(),
            url_template.name(),
            method.name()
        );

        if handler_name.is_empty() {
            let message = format!("Задайте обработчик http-сервиса \"{}\"", service_path);
            diagnostics.push(Diagnostic {
                code,
                message,
                severity: ctx.severity(code),
                range: header_range,
                tags: ctx.tags(code),
                fixes: vec![],
            });
            continue;
        }

        let name = Name::new(handler_name);
        let method_symbol = symbol_tree.find_method(&name);

        match method_symbol {
            None => {
                let message = format!(
                    "Создайте функцию-обработчик \"{}\" или исправьте некорректный обработчик http-сервиса \"{}\"",
                    handler_name, service_path
                );
                diagnostics.push(Diagnostic {
                    code,
                    message,
                    severity: ctx.severity(code),
                    range: header_range,
                    tags: ctx.tags(code),
                    fixes: vec![],
                });
            }
            Some(method_sym) => {
                let param_count = method_sym.params.len();
                if param_count != 1 {
                    let message = format!(
                        "Задайте всего один параметр у обработчика \"{}\" для http-сервиса \"{}\"",
                        handler_name, service_path
                    );

                    let range = get_method_name_range(&item_tree, method_sym.id.local_id)
                        .unwrap_or(header_range);

                    diagnostics.push(Diagnostic {
                        code,
                        message,
                        severity: ctx.severity(code),
                        range,
                        tags: ctx.tags(code),
                        fixes: vec![],
                    });
                }
            }
        }
    }

    diagnostics
}

fn get_method_name_range(item_tree: &hir::ItemTree, local_id: hir::MethodKey) -> Option<TextRange> {
    let item = item_tree.item_of(local_id)?;
    match item {
        ModItem::Function(func_idx) => Some(item_tree.function(*func_idx).name_range),
        ModItem::Procedure(proc_idx) => Some(item_tree.procedure(*proc_idx).name_range),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DiagnosticsConfig;
    use std::sync::Arc;
    fn make_http_service_metadata(http_service: bsl_metadata::HTTPService) -> ModuleMetadata {
        ModuleMetadata {
            module_type: bsl_metadata::ModuleType::HTTPServiceModule,
            execution_context: None,
            common_module: None,
            common_command: None,
            sequence: None,
            settings_storage: None,
            mdo: None,
            register: None,
            http_service: Some(Arc::new(http_service)),
            form: None,
            web_service: None,
            integration_service: None,
        }
    }

    #[test]
    fn test_missing_handler() {
        let method = bsl_metadata::HTTPServiceMethodBuilder::new()
            .name("GET")
            .http_method("GET")
            .handler("")
            .build();

        let template = bsl_metadata::HTTPServiceURLTemplateBuilder::new()
            .name("URLTemplate1")
            .template("/test")
            .add_method(method)
            .build();

        let http_service = bsl_metadata::HTTPServiceBuilder::new()
            .name("HTTPСервис1")
            .root_url("/api")
            .add_url_template(template)
            .build();

        let metadata = make_http_service_metadata(http_service);
        let file_text = "Процедура Тест()\nКонецПроцедуры";
        let diagnostics =
            crate::test_utils::check_metadata_diagnostic(metadata, file_text, from_metadata);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Задайте обработчик"));
    }

    #[test]
    fn test_handler_not_found() {
        let method = bsl_metadata::HTTPServiceMethodBuilder::new()
            .name("GET")
            .http_method("GET")
            .handler("НесуществующийОбработчик")
            .build();

        let template = bsl_metadata::HTTPServiceURLTemplateBuilder::new()
            .name("URLTemplate1")
            .template("/test")
            .add_method(method)
            .build();

        let http_service = bsl_metadata::HTTPServiceBuilder::new()
            .name("HTTPСервис1")
            .root_url("/api")
            .add_url_template(template)
            .build();

        let metadata = make_http_service_metadata(http_service);
        let file_text = "Процедура Тест()\nКонецПроцедуры";
        let diagnostics =
            crate::test_utils::check_metadata_diagnostic(metadata, file_text, from_metadata);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Создайте функцию-обработчик"));
    }

    #[test]
    fn test_incorrect_handler_params() {
        let method = bsl_metadata::HTTPServiceMethodBuilder::new()
            .name("POST")
            .http_method("POST")
            .handler("НеверныйОбработчик")
            .build();

        let template = bsl_metadata::HTTPServiceURLTemplateBuilder::new()
            .name("URLTemplate1")
            .template("/test")
            .add_method(method)
            .build();

        let http_service = bsl_metadata::HTTPServiceBuilder::new()
            .name("HTTPСервис1")
            .root_url("/api")
            .add_url_template(template)
            .build();

        let metadata = make_http_service_metadata(http_service);
        let file_text = "Функция НеверныйОбработчик(Запрос, ВторойПараметр)\n\tВозврат Неопределено;\nКонецФункции";
        let diagnostics =
            crate::test_utils::check_metadata_diagnostic(metadata, file_text, from_metadata);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Задайте всего один параметр"));
    }

    #[test]
    fn test_valid_handler() {
        let method = bsl_metadata::HTTPServiceMethodBuilder::new()
            .name("GET")
            .http_method("GET")
            .handler("ВерныйОбработчик")
            .build();

        let template = bsl_metadata::HTTPServiceURLTemplateBuilder::new()
            .name("URLTemplate1")
            .template("/test")
            .add_method(method)
            .build();

        let http_service = bsl_metadata::HTTPServiceBuilder::new()
            .name("HTTPСервис1")
            .root_url("/api")
            .add_url_template(template)
            .build();

        let metadata = make_http_service_metadata(http_service);
        let file_text = "Функция ВерныйОбработчик(Запрос)\n\tВозврат Неопределено;\nКонецФункции";
        let diagnostics =
            crate::test_utils::check_metadata_diagnostic(metadata, file_text, from_metadata);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_not_http_service_module() {
        let metadata = ModuleMetadata {
            module_type: bsl_metadata::ModuleType::CommonModule,
            execution_context: None,
            common_module: None,
            common_command: None,
            sequence: None,
            settings_storage: None,
            mdo: None,
            register: None,
            http_service: None,
            web_service: None,
            integration_service: None,
            form: None,
        };

        let file_text = "Процедура Тест()\nКонецПроцедуры";
        let diagnostics =
            crate::test_utils::check_metadata_diagnostic(metadata, file_text, from_metadata);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_disabled_diagnostic() {
        let method = bsl_metadata::HTTPServiceMethodBuilder::new()
            .name("GET")
            .http_method("GET")
            .handler("")
            .build();

        let template = bsl_metadata::HTTPServiceURLTemplateBuilder::new()
            .name("URLTemplate1")
            .template("/test")
            .add_method(method)
            .build();

        let http_service = bsl_metadata::HTTPServiceBuilder::new()
            .name("HTTPСервис1")
            .root_url("/api")
            .add_url_template(template)
            .build();

        let metadata = make_http_service_metadata(http_service);

        let mut config = DiagnosticsConfig::default();
        config.disabled.push(DiagnosticCode::WrongHttpServiceHandler);

        let file_text = "Процедура Тест()\nКонецПроцедуры";
        let diagnostics = crate::test_utils::check_metadata_diagnostic_with_config(
            metadata,
            file_text,
            config,
            from_metadata,
        );

        assert!(diagnostics.is_empty());
    }

    // Wave 2d RED: locks the contract that the handler is recognized from typed module
    // metadata populated by the bootstrapped substrate (no full Configuration side-path).
    // RED-fails until `ide_db::metadata::HTTPServiceEntry` exists and the substrate-driven
    // module-metadata builder populates `ModuleMetadata::http_service` from it.
    #[test]
    fn test_handler_recognized_from_substrate_populated_metadata() {
        let _substrate_entry = ide_db::metadata::HTTPServiceEntry {
            name: "HTTPСервис1".to_string(),
            main: vfs::FileId(0),
            module_file: None,
        };

        let method = bsl_metadata::HTTPServiceMethodBuilder::new()
            .name("GET")
            .http_method("GET")
            .handler("ВерныйОбработчик")
            .build();

        let template = bsl_metadata::HTTPServiceURLTemplateBuilder::new()
            .name("URLTemplate1")
            .template("/test")
            .add_method(method)
            .build();

        let http_service = bsl_metadata::HTTPServiceBuilder::new()
            .name("HTTPСервис1")
            .root_url("/api")
            .add_url_template(template)
            .build();

        let metadata = make_http_service_metadata(http_service);
        let file_text = "Функция ВерныйОбработчик(Запрос)\n\tВозврат Неопределено;\nКонецФункции";
        let diagnostics =
            crate::test_utils::check_metadata_diagnostic(metadata, file_text, from_metadata);

        assert!(
            diagnostics.is_empty(),
            "handler recognized from substrate-populated metadata must not fire: {diagnostics:?}"
        );
    }
}
