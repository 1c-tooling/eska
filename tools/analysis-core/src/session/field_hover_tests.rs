//! Positional field cards share native receiver resolution and authoritative current metadata.

use super::*;
use crate::{
    HoverKind, Locale, QueryCancellation, QueryError, SymbolHover,
    session::tests::{owner, support::TestDirectory},
};
use std::{fs, path::PathBuf};

const MODULE: &str = "CommonModules/Tools/Module.bsl";
const CATALOG: &str = "Catalogs/Items/object.yaml";
const NEIGHBOR: &str = "Catalogs/Neighbor/object.yaml";
const BODY: &str = "\u{feff}// Я😀\r\nProcedure Run()\r\n Item=Catalogs.Items.CreateItem();\r\n Alias=Item;\r\n Я=\"😀\"; Result=Alias.CurrentЯ;\r\nEndProcedure\r\n";

/// Real imported declarations give the cold target and unrelated neighbor independent current slots.
fn project(body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    let native = directory.0.join("native");
    let header = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        header.replace(
            "</ChildObjects>",
            "<Catalog>Items</Catalog><Catalog>Neighbor</Catalog></ChildObjects>",
        ),
    )
    .unwrap();
    fs::create_dir(native.join("Catalogs")).unwrap();
    for (name, uuid, fields) in [
        (
            "Items",
            "80000000-0000-4000-8000-000000000001",
            "<ChildObjects><Attribute uuid=\"80000000-0000-4000-8000-000000000003\"><Properties><Name>CurrentЯ</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects>",
        ),
        ("Neighbor", "80000000-0000-4000-8000-000000000002", ""),
    ] {
        fs::write(native.join(format!("Catalogs/{name}.xml")), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><Catalog uuid=\"{uuid}\"><Properties><Name>{name}</Name></Properties>{fields}</Catalog></MetaDataObject>")).unwrap();
    }
    let source = root.join("src-with-fields");
    eska::project_core::source_format::import_designer_queries(&native, &source).unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// Native dependency discovery prepares the exact selected member before the read-only public card query.
fn hover(
    session: &mut AnalysisSession,
    offset: u32,
    locale: Locale,
) -> Result<Option<SymbolHover>, QueryError> {
    session.prepare_symbol_hover_metadata(
        &owner(MODULE),
        session.revision(),
        offset,
        &InputRequest::default(),
        &QueryCancellation::default(),
    )?;
    Ok(session
        .module_symbol_hover(
            &owner(MODULE),
            session.revision(),
            offset,
            locale,
            &QueryCancellation::default(),
        )?
        .value)
}

/// The first field hover resolves a reaching alias without loading an unrelated configured object.
#[test]
fn field_hover_is_cold_typed_localized_and_exact_at_both_identifier_boundaries() {
    let (_directory, source, mut session) = project(BODY);
    let start = u32::try_from(BODY.rfind("CurrentЯ").unwrap()).unwrap();
    let end = start + u32::try_from("CurrentЯ".len()).unwrap();
    assert!(
        session
            .module_symbol_hover(
                &owner(MODULE),
                session.revision(),
                start,
                Locale::En,
                &QueryCancellation::default()
            )
            .unwrap()
            .value
            .is_none()
    );
    for (locale, expected) in [(Locale::En, "String"), (Locale::Ru, "Строка")] {
        for offset in [start, end] {
            let card = hover(&mut session, offset, locale)
                .unwrap()
                .unwrap_or_else(|| {
                    panic!(
                        "Missing field card: target={:?}, resources={:?}",
                        session.selected_root.owners[&owner(CATALOG)].model.state,
                        session.resources()
                    )
                });
            assert_eq!(card.kind, HoverKind::Field);
            assert_eq!(card.symbol, "CurrentЯ");
            assert_eq!(card.return_type.as_deref(), Some(expected));
            assert_eq!((card.start_byte, card.end_byte), (start, end));
            assert!(card.declaration.is_none());
            assert!(card.documentation.is_none());
        }
    }
    assert!(matches!(
        session.selected_root.owners[&owner(NEIGHBOR)].model.state,
        ModelState::Pending
    ));
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), BODY);
}

/// Current type edits and failures replace the card; watch never replaces the editor's authoritative input.
#[test]
fn field_hover_tracks_type_invalid_repair_watch_and_close_without_saved_fallback() {
    let (_directory, source, mut session) = project(BODY);
    let at = u32::try_from(BODY.rfind("CurrentЯ").unwrap()).unwrap();
    assert_eq!(
        hover(&mut session, at, Locale::En)
            .unwrap()
            .unwrap()
            .return_type
            .as_deref(),
        Some("String")
    );
    let path = owner(CATALOG);
    let saved = fs::read_to_string(source.join(CATALOG)).unwrap();
    let changed = saved.replace("xs:string", "xs:boolean");
    assert_ne!(changed, saved);
    let request = InputRequest::default();
    let earlier = session.revision();
    session
        .apply_overlay(&path, earlier, 1, &changed, &request)
        .unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&path), session.revision())
        .unwrap();
    assert_eq!(
        hover(&mut session, at, Locale::Ru)
            .unwrap()
            .unwrap()
            .return_type
            .as_deref(),
        Some("Булево")
    );
    assert!(matches!(
        session.module_symbol_hover(
            &owner(MODULE),
            earlier,
            at,
            Locale::En,
            &QueryCancellation::default()
        ),
        Err(QueryError::Request(_))
    ));
    session
        .apply_overlay(&path, session.revision(), 2, "broken: [", &request)
        .unwrap();
    assert!(hover(&mut session, at, Locale::En).unwrap().is_none());
    session
        .apply_overlay(&path, session.revision(), 3, &changed, &request)
        .unwrap();
    assert_eq!(
        hover(&mut session, at, Locale::En)
            .unwrap()
            .unwrap()
            .return_type
            .as_deref(),
        Some("Boolean")
    );
    session
        .close_overlay(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(
        hover(&mut session, at, Locale::En)
            .unwrap()
            .unwrap()
            .return_type
            .as_deref(),
        Some("String")
    );
    assert_eq!(fs::read_to_string(source.join(CATALOG)).unwrap(), saved);
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), BODY);
}

/// A missing field and a locally owned receiver cannot borrow a configured property with the same spelling.
#[test]
fn field_hover_keeps_missing_and_shadowed_receivers_unresolved() {
    let missing = BODY.replace("Alias.CurrentЯ", "Alias.MissingЯ");
    let (_directory, _source, mut session) = project(&missing);
    assert!(
        hover(
            &mut session,
            u32::try_from(missing.find("MissingЯ").unwrap()).unwrap(),
            Locale::En
        )
        .unwrap()
        .is_none()
    );
    let body = "Procedure Run(Catalogs)\n Item=Catalogs.Items.CreateItem();\n Value=Item.CurrentЯ;\nEndProcedure\n";
    let (_directory, _source, mut session) = project(body);
    assert!(
        hover(
            &mut session,
            u32::try_from(body.find("CurrentЯ").unwrap()).unwrap(),
            Locale::En
        )
        .unwrap()
        .is_none()
    );
    assert!(matches!(
        session.selected_root.owners[&owner(CATALOG)].model.state,
        ModelState::Pending
    ));
}

/// Cold discovery follows only reaching values and the receiver/callee spine, with bounded cyclic aliases.
#[test]
fn field_hover_cold_discovery_rejects_overwritten_cycles_arguments_and_other_procedures() {
    for body in [
        "Procedure Run()\n Item=Catalogs.Items.CreateItem();\n Alias=Item;\n Alias=1;\n Value=Alias.CurrentЯ;\nEndProcedure\n",
        "Procedure Run()\n Alias=Alias;\n Value=Alias.CurrentЯ;\nEndProcedure\nProcedure Other()\n Item=Catalogs.Items.CreateItem();\nEndProcedure\n",
        "Procedure Run()\n A=B;\n B=A;\n Value=A.CurrentЯ;\nEndProcedure\nProcedure Other()\n Item=Catalogs.Items.CreateItem();\nEndProcedure\n",
        "Function Opaque(Value)\nEndFunction\nProcedure Run()\n Value=Opaque(Catalogs.Items.CreateItem()).CurrentЯ;\nEndProcedure\n",
        "Procedure Run(Flag)\n If Flag Then\n  Alias=Catalogs.Items.CreateItem();\n Else\n  Alias=New Query;\n EndIf;\n Value=Alias.CurrentЯ;\nEndProcedure\n",
    ] {
        let (_directory, _source, mut session) = project(body);
        let at = u32::try_from(body.find("CurrentЯ").unwrap()).unwrap();
        assert!(
            hover(&mut session, at, Locale::En).unwrap().is_none(),
            "{body}"
        );
        assert!(
            matches!(
                session.selected_root.owners[&owner(CATALOG)].model.state,
                ModelState::Pending
            ),
            "{body}"
        );
    }
}
