//! Cold property chains prepare native reference owners without borrowing saved or unrelated metadata.

use super::*;
use crate::{
    HoverKind, Locale, QueryCancellation, SymbolHover,
    session::tests::{owner, support::TestDirectory},
};
use std::{fs, path::PathBuf};

const MODULE: &str = "CommonModules/Tools/Module.bsl";
const ITEMS: &str = "Catalogs/Items/object.yaml";
const NEIGHBOR: &str = "Catalogs/Neighbor/object.yaml";
const THIRD: &str = "Catalogs/Third/object.yaml";
const UNRELATED: &str = "Catalogs/Unrelated/object.yaml";
const BODY: &str = "\u{feff}// Я😀\r\nProcedure Run()\r\n Item=Catalogs.Items.CreateItem();\r\n Alias=Item.Child;\r\n Value=Alias.CurrentЯ;\r\nEndProcedure\r\n";
const DEEP_BODY: &str = "\u{feff}// Я😀\r\nProcedure Run()\r\n Item=Catalogs.Items.CreateItem();\r\n Alias=Item.Child;\r\n Value=Alias.GrandChild.CurrentЯ;\r\nEndProcedure\r\n";

/// The original two-owner case keeps its own first-request baseline.
fn project() -> (TestDirectory, PathBuf, AnalysisSession) {
    project_with_body(BODY)
}

/// Import actual reference-typed metadata so each receiver hop has a separate current owner slot.
fn project_with_body(body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    let native = directory.0.join("native");
    let header = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        header.replace(
            "</ChildObjects>",
            "<Catalog>Items</Catalog><Catalog>Neighbor</Catalog><Catalog>Third</Catalog><Catalog>Unrelated</Catalog></ChildObjects>",
        ),
    )
    .unwrap();
    fs::create_dir(native.join("Catalogs")).unwrap();
    for (name, uuid, fields) in [
        (
            "Items",
            "81000000-0000-4000-8000-000000000001",
            "<ChildObjects><Attribute uuid=\"81000000-0000-4000-8000-000000000004\"><Properties><Name>Child</Name><Type><v8:Type>cfg:CatalogRef.Neighbor</v8:Type></Type></Properties></Attribute></ChildObjects>",
        ),
        (
            "Neighbor",
            "81000000-0000-4000-8000-000000000002",
            "<ChildObjects><Attribute uuid=\"81000000-0000-4000-8000-000000000005\"><Properties><Name>CurrentЯ</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute><Attribute uuid=\"81000000-0000-4000-8000-000000000006\"><Properties><Name>GrandChild</Name><Type><v8:Type>cfg:CatalogRef.Third</v8:Type></Type></Properties></Attribute></ChildObjects>",
        ),
        (
            "Third",
            "81000000-0000-4000-8000-000000000007",
            "<ChildObjects><Attribute uuid=\"81000000-0000-4000-8000-000000000008\"><Properties><Name>CurrentЯ</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects>",
        ),
        ("Unrelated", "81000000-0000-4000-8000-000000000003", ""),
    ] {
        fs::write(native.join(format!("Catalogs/{name}.xml")), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"><Catalog uuid=\"{uuid}\"><Properties><Name>{name}</Name></Properties>{fields}</Catalog></MetaDataObject>")).unwrap();
    }
    let source = root.join("src-with-field-chain");
    eska::project_core::source_format::import_designer_queries(&native, &source).unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// A single public preparation request must reach the same native card as explicitly loaded current owners.
fn hover(session: &mut AnalysisSession) -> Option<SymbolHover> {
    hover_body(session, BODY)
}

/// Both chain depths retain the hovered member's actual byte span.
fn hover_body(session: &mut AnalysisSession, body: &str) -> Option<SymbolHover> {
    let offset = u32::try_from(body.find("CurrentЯ").unwrap()).unwrap();
    session
        .prepare_symbol_hover_metadata(
            &owner(MODULE),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    session
        .module_symbol_hover(
            &owner(MODULE),
            session.revision(),
            offset,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
}

/// Every required reference hop is ready after the first hover while an unrelated descriptor stays unread.
#[test]
fn first_cold_field_chain_hover_matches_the_warm_native_baseline() {
    let (_warm_directory, _warm_source, mut warm) = project();
    for path in [ITEMS, NEIGHBOR] {
        warm.load_saved(&owner(path), warm.revision(), &InputRequest::default())
            .unwrap();
    }
    let expected = hover(&mut warm).expect("warm native reference field");
    assert_eq!(expected.kind, HoverKind::Field);
    assert_eq!(expected.symbol, "CurrentЯ");
    assert_eq!(expected.return_type.as_deref(), Some("String"));
    let (_cold_directory, _cold_source, mut cold) = project();
    for path in [ITEMS, NEIGHBOR, UNRELATED] {
        assert!(matches!(
            cold.selected_root.owners[&owner(path)].model.state,
            ModelState::Pending
        ));
    }
    assert_eq!(hover(&mut cold), Some(expected));
    for path in [ITEMS, NEIGHBOR] {
        assert!(!matches!(
            cold.selected_root.owners[&owner(path)].model.state,
            ModelState::Pending
        ));
    }
    assert!(matches!(
        cold.selected_root.owners[&owner(UNRELATED)].model.state,
        ModelState::Pending
    ));
    assert!(matches!(
        cold.selected_root.owners[&owner(THIRD)].model.state,
        ModelState::Pending
    ));
}

/// An unknown final receiver advances through a typed intermediate reference without reading sibling owners.
#[test]
fn first_cold_deeper_field_chain_hover_matches_the_warm_native_baseline() {
    let (_warm_directory, _warm_source, mut warm) = project_with_body(DEEP_BODY);
    for path in [ITEMS, NEIGHBOR, THIRD] {
        warm.load_saved(&owner(path), warm.revision(), &InputRequest::default())
            .unwrap();
    }
    let expected = hover_body(&mut warm, DEEP_BODY).expect("warm native deeper reference field");
    assert_eq!(expected.kind, HoverKind::Field);
    assert_eq!(expected.return_type.as_deref(), Some("String"));
    let (_cold_directory, _cold_source, mut cold) = project_with_body(DEEP_BODY);
    for path in [ITEMS, NEIGHBOR, THIRD, UNRELATED] {
        assert!(matches!(
            cold.selected_root.owners[&owner(path)].model.state,
            ModelState::Pending
        ));
    }
    assert_eq!(hover_body(&mut cold, DEEP_BODY), Some(expected));
    for path in [ITEMS, NEIGHBOR, THIRD] {
        assert!(!matches!(
            cold.selected_root.owners[&owner(path)].model.state,
            ModelState::Pending
        ));
    }
    assert!(matches!(
        cold.selected_root.owners[&owner(UNRELATED)].model.state,
        ModelState::Pending
    ));
}

/// A current scalar or invalid reference declaration cannot resurrect the saved reference target's field.
#[test]
fn field_chain_hover_preserves_current_reference_type_and_failure_authority() {
    for body in [BODY, DEEP_BODY] {
        let (_directory, source, mut session) = project_with_body(body);
        let path = owner(ITEMS);
        let saved = fs::read_to_string(source.join(ITEMS)).unwrap();
        let scalar = saved.replace("cfg:CatalogRef.Neighbor", "xs:boolean");
        assert_ne!(scalar, saved);
        let request = InputRequest::default();
        for (version, text) in [(1, scalar.as_str()), (2, "broken: [")] {
            session
                .apply_overlay(&path, session.revision(), version, text, &request)
                .unwrap();
            assert!(hover_body(&mut session, body).is_none(), "{body}");
            for target in [NEIGHBOR, THIRD] {
                assert!(
                    matches!(
                        session.selected_root.owners[&owner(target)].model.state,
                        ModelState::Pending
                    ),
                    "{body}"
                );
            }
        }
        session
            .apply_overlay(&path, session.revision(), 3, &saved, &request)
            .unwrap();
        let repaired = hover_body(&mut session, body).expect("repaired native reference field");
        assert_eq!(repaired.return_type.as_deref(), Some("String"));
        session
            .close_overlay(&path, session.revision(), &request)
            .unwrap();
        assert_eq!(hover_body(&mut session, body), Some(repaired));
        assert!(matches!(
            session.selected_root.owners[&owner(UNRELATED)].model.state,
            ModelState::Pending
        ));
        assert_eq!(fs::read_to_string(source.join(ITEMS)).unwrap(), saved);
    }
}
