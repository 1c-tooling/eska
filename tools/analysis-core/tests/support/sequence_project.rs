//! Native sequence projects use the common importer and keep every test source under its owned directory.

use std::{fs, path::Path};

/// Add a sequence and a real record-set module, optionally borrowing the original sequence identity.
pub fn add_sequence(
    root: &Path,
    native: &Path,
    record_set_body: &str,
    extension: bool,
    uuid: &str,
    extra: &str,
) {
    let config = native.join("Configuration.xml");
    let objects = if extension {
        "<Sequence>Chain</Sequence>"
    } else {
        "<Sequence>Chain</Sequence><Catalog>Items</Catalog><Document>Orders</Document>"
    };
    let mut text = fs::read_to_string(&config)
        .unwrap()
        .replace("</ChildObjects>", &format!("{objects}</ChildObjects>"));
    if extension {
        text=text.replace("<Name>SyntaxFixture</Name>","<Name>Extension</Name><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>").replace("10000000-0000-4000-8000-000000000001",&uuid.replacen("70000000","10000000",1));
        let module = native.join("CommonModules/Tools.xml");
        let common = fs::read_to_string(&module).unwrap().replace(
            "20000000-0000-4000-8000-000000000001",
            &uuid.replacen("70000000", "20000000", 1),
        );
        fs::write(module, common).unwrap();
    }
    fs::write(config, text).unwrap();
    if !extension {
        for (family, kind, name, prefix) in [
            ("Catalogs", "Catalog", "Items", "30000000"),
            ("Documents", "Document", "Orders", "40000000"),
        ] {
            fs::create_dir_all(native.join(family)).unwrap();
            let object_uuid = uuid.replacen("70000000", prefix, 1);
            fs::write(native.join(format!("{family}/{name}.xml")),format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><{kind} uuid='{object_uuid}'><Properties><Name>{name}</Name></Properties><ChildObjects/></{kind}></MetaDataObject>")).unwrap();
        }
    }
    fs::create_dir_all(native.join("Sequences/Chain/Ext")).unwrap();
    let borrowed = extra.contains("<ObjectBelonging>Adopted</ObjectBelonging>");
    let properties = if borrowed {
        ""
    } else {
        "<Documents><r:Item>Document.Orders</r:Item></Documents><RegisterRecords/>"
    };
    let dimension_uuid = uuid.replacen("70000000", "80000000", 1);
    let children = if borrowed {
        String::new()
    } else {
        format!(
            "<Dimension uuid='{dimension_uuid}'><Properties><Name>KeyЯ</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Dimension>"
        )
    };
    fs::write(native.join("Sequences/Chain.xml"),format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:r='http://v8.1c.ru/8.3/xcf/readable' xmlns:v8='http://v8.1c.ru/8.1/data/core' xmlns:xs='http://www.w3.org/2001/XMLSchema' version='2.20'><Sequence uuid='{uuid}'><Properties><Name>Chain</Name>{properties}{extra}</Properties><ChildObjects>{children}</ChildObjects></Sequence></MetaDataObject>")).unwrap();
    fs::write(
        native.join("Sequences/Chain/Ext/RecordSetModule.bsl"),
        record_set_body,
    )
    .unwrap();
    eska::project_core::source_format::import_designer_queries(
        native,
        &root.join("src-with-sequences"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap().replace(
        "source_format='eska'",
        "source_format='eska'\nsource='src-with-sequences'",
    );
    fs::write(
        root.join("eska.toml"),
        if extension {
            config.replace("type='configuration'", "type='extension'")
        } else {
            config
        },
    )
    .unwrap();
}
