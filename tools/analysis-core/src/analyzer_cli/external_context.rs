//! External own-context capabilities never imply an available owning configuration.

use serde_json::{Value, json};

use crate::AnalysisScope;

/// Scope-specific additions preserve existing CF and `LocalSyntax` protocol shapes.
pub(super) fn advertise(result: &mut Value, scope: AnalysisScope) {
    if scope != AnalysisScope::CanonicalExternal {
        return;
    }
    let capabilities = &mut result["capabilities"];
    if let Some(values) = capabilities.as_object_mut() {
        values.remove("diagnosticProvider");
    }
    let eska = &mut capabilities["experimental"]["eska"];
    eska["metadataContextComplete"] = json!(false);
    eska["configurationContext"] = json!({
        "available":false,
        "requiredFor":"project-semantic-diagnostics-and-foreign-metadata",
    });
    eska["managerModuleCompletion"] = json!(false);
    eska["commonModuleCompletion"] = json!(false);
    eska["analysis"] = json!({
        "available":false,
        "profile":"semantic",
        "code":"analysis_configuration_context_required",
        "metadataEnabled":true,
        "semanticEnabled":false,
        "profiles":["metadata"],
    });
    eska["metadata"]["diagnostics"] = json!({"method":"textDocument/diagnostic","scope":"declared-owner-yaml","profile":"metadata","identifier":"eska-semantic","cancellable":true});
    for name in ["languageMethods", "cancellableMethods"] {
        if let Some(methods) = eska[name].as_array_mut() {
            methods.retain(|method| method != "textDocument/diagnostic");
        }
    }
}

/// Actual own models can be ready while a consumer's whole metadata context remains explicitly incomplete.
pub(super) fn coverage(context: &mut Value, scope: AnalysisScope) {
    if scope == AnalysisScope::CanonicalExternal {
        context["analysis_scope"] = json!(scope);
        context["coverage"]["metadata_context_complete"] = json!(false);
    }
}
