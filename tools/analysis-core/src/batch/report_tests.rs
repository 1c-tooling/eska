//! Native header repair must not weaken the batch boundary for genuinely invalid coordinates.

use super::*;

/// Split UTF-8 and out-of-bounds native endpoints remain structured failures before conversion.
#[test]
fn invalid_native_diagnostic_ranges_remain_rejected() {
    let text = "\u{feff}\r\n\r\n#Область Я😀\r\n";
    let source = SourcePath::parse("CommonForms/Dialog/Ext/Form/Module.bsl").unwrap();
    let past_end = u32::try_from(text.len()).unwrap() + 1;
    for (start, end) in [(0, 9), (9, 10), (0, past_end)] {
        let diagnostic = Diagnostic {
            code: DiagnosticCode::WrongDataPathForFormElements,
            message: "Invalid native header coordinates.".into(),
            severity: ide::Severity::Critical,
            range: ide_db::TextRange::new(start.into(), end.into()),
            tags: Vec::new(),
            fixes: Vec::new(),
        };
        assert_eq!(
            finding(&source, &diagnostic, text),
            Err(BatchError::DiagnosticRangeInvalid {
                source: source.clone(),
                code: "WrongDataPathForFormElements".into(),
            })
        );
    }
}
