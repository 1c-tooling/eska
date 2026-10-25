use super::*;

/// A saved manual list is editable; an automatic list with only `MainTable` never gets a fake query.
#[test]
fn query_rpc_reads_and_plans_manual_dynamic_list_without_inventing_automatic_text() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    std::fs::write(
        &root,
        std::fs::read_to_string(&root).unwrap().replace(
            "<ChildObjects>",
            "<ChildObjects><CommonForm>Queries</CommonForm>",
        ),
    )
    .unwrap();
    std::fs::create_dir_all(source.join("CommonForms/Queries/Ext")).unwrap();
    std::fs::write(source.join("CommonForms/Queries.xml"),"<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><CommonForm uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Queries</Name></Properties></CommonForm></MetaDataObject>").unwrap();
    let payload = "<Form xmlns='http://v8.1c.ru/8.3/xcf/logform' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'><Attributes><Attribute name='Список' id='1'><Settings xsi:type='DynamicList'><ManualQuery>true</ManualQuery><QueryText>ВЫБРАТЬ &quot;😀&quot;</QueryText></Settings></Attribute><Attribute name='Auto' id='2'><Settings xsi:type='DynamicList'><ManualQuery>false</ManualQuery><MainTable>Catalog.Test</MainTable></Settings></Attribute></Attributes></Form>";
    let path = "CommonForms/Queries/Ext/Form.xml";
    std::fs::write(source.join(path), payload).unwrap();
    let mut expected = None;
    for locale in ["ru-RU", "en-US"] {
        let mut client = Client::new(locale);
        initialize(&mut client, locale);
        let workspace = client.open(&directory.0);
        let key = key(&workspace, 0);
        let opened = client.ok("query/open",with(&key,json!({"node":{"kind":"object","objectId":"common-form:Queries"},"source":{"value":path,"encoding":"utf-8"},"version":"1"})));
        let revision = binding(&key, &opened);
        let list = client.ok("query/list", revision.clone());
        assert_eq!(list["queries"].as_array().unwrap().len(), 1);
        assert_eq!(
            list["queries"][0]["identity"],
            json!({"kind":"dynamicList","name":"Список","id":"1"})
        );
        let read = client.ok(
            "query/read",
            with(&revision, json!({"query":list["queries"][0]["identity"]})),
        );
        assert_eq!(read["query"]["text"], "ВЫБРАТЬ \"😀\"");
        let plan = client.ok(
            "query/plan",
            with(
                &revision,
                json!({"query":read["query"]["identity"],"text":"ВЫБРАТЬ A < 2 & B","comments":[]}),
            ),
        );
        assert_eq!(
            apply(payload, &plan),
            payload.replace("ВЫБРАТЬ &quot;😀&quot;", "ВЫБРАТЬ A &lt; 2 &amp; B")
        );
        // A non-root descriptor is outside the application fingerprint; UUID/generation remain valid.
        let descriptor = source.join("CommonForms/Queries.xml");
        let original = std::fs::read_to_string(&descriptor).unwrap();
        std::fs::write(&descriptor, format!("{original}\r\n<!-- external -->")).unwrap();
        let stale_read = client.request(
            "query/read",
            with(&revision, json!({"query":list["queries"][0]["identity"]})),
        );
        let stale_plan = client.request(
            "query/plan",
            with(
                &revision,
                json!({"query":read["query"]["identity"],"text":"SELECT stale","comments":[]}),
            ),
        );
        for result in [&stale_read, &stale_plan] {
            assert_eq!(result["error"]["data"]["kind"], "query_failed", "{result}");
            assert_eq!(
                result["error"]["data"]["details"]["reason"],
                "owner_changed"
            );
        }
        std::fs::write(&descriptor, original).unwrap();
        assert_eq!(
            client.ok(
                "query/read",
                with(&revision, json!({"query":list["queries"][0]["identity"]})),
            )["query"]["text"],
            read["query"]["text"]
        );
        let result = (
            plan,
            stale_read["error"].clone(),
            stale_plan["error"].clone(),
        );
        if let Some(expected) = &expected {
            assert_eq!(&result, expected);
        } else {
            expected = Some(result);
        }
        client.finish();
    }
    assert_eq!(std::fs::read_to_string(source.join(path)).unwrap(), payload);
}

/// The physical XML payload belongs to a declared `CommonTemplate`, not to a basename glob.
#[test]
fn query_rpc_lists_scd_datasets_and_plans_only_the_selected_xml_scalar() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    std::fs::write(
        &root,
        std::fs::read_to_string(&root).unwrap().replace(
            "<ChildObjects>",
            "<ChildObjects><CommonTemplate>Queries</CommonTemplate>",
        ),
    )
    .unwrap();
    std::fs::create_dir_all(source.join("CommonTemplates/Queries/Ext")).unwrap();
    std::fs::write(source.join("CommonTemplates/Queries.xml"),"<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><CommonTemplate uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Queries</Name></Properties></CommonTemplate></MetaDataObject>").unwrap();
    let payload = "\u{feff}<DataCompositionSchema xmlns='http://v8.1c.ru/8.1/data-composition-system/schema' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'><dataSet xsi:type='DataSetQuery'><name>A</name><query><!--keep--><![CDATA[ВЫБРАТЬ \"😀\"]]></query></dataSet><dataSet xsi:type='DataSetQuery'><name>B</name><query>SELECT 2</query></dataSet></DataCompositionSchema>";
    let path = "CommonTemplates/Queries/Ext/Template.xml";
    std::fs::write(source.join(path), payload).unwrap();
    let before = bytes(&directory.0);
    let mut client = Client::new("ru-RU");
    initialize(&mut client, "ru-RU");
    let workspace = client.open(&directory.0);
    let key = key(&workspace, 0);
    let opened=client.ok("query/open",with(&key,json!({"node":{"kind":"object","objectId":"common-template:Queries"},"source":{"value":path,"encoding":"utf-8"},"version":"1"})));
    let revision = binding(&key, &opened);
    let first = client.ok("query/list", with(&revision, json!({"limit":1})));
    assert_eq!(
        first["queries"][0]["identity"],
        json!({"kind":"dataSet","path":["A"]})
    );
    assert_eq!(first["nextOffset"], 1);
    let second = client.ok("query/list", with(&revision, json!({"offset":1})));
    assert_eq!(second["queries"][0]["identity"]["path"], json!(["B"]));
    let read = client.ok(
        "query/read",
        with(&revision, json!({"query":first["queries"][0]["identity"]})),
    );
    let unchanged = client.ok(
        "query/plan",
        with(
            &revision,
            json!({"query":read["query"]["identity"],"text":read["query"]["text"],"comments":[]}),
        ),
    );
    assert_eq!(apply(payload, &unchanged), payload);
    let plan = client.ok(
        "query/plan",
        with(
            &revision,
            json!({"query":read["query"]["identity"],"text":"ВЫБРАТЬ A < 2 & B","comments":[]}),
        ),
    );
    assert_eq!(
        apply(payload, &plan),
        payload.replace("<![CDATA[ВЫБРАТЬ \"😀\"]]>", "ВЫБРАТЬ A &lt; 2 &amp; B")
    );
    assert_eq!(bytes(&directory.0), before);
    for (ending, newline) in [("lf", "\n"), ("crlf", "\r\n")] {
        let physical = client.ok("query/plan", with(&revision, json!({"query":read["query"]["identity"],"text":"SELECT 1\nFROM T","comments":[],"lineEnding":ending})));
        assert_eq!(
            apply(payload, &physical),
            payload.replace(
                "<![CDATA[ВЫБРАТЬ \"😀\"]]>",
                &format!("SELECT 1{newline}FROM T")
            )
        );
        assert_eq!(physical["projectedQuery"]["text"], "SELECT 1\nFROM T");
    }
    for ending in [json!("unknown"), Value::Null, json!(true)] {
        assert_eq!(client.request("query/plan", with(&revision,json!({"query":read["query"]["identity"],"text":"SELECT 1","comments":[],"lineEnding":ending})))["error"]["code"], -32602);
    }
    client.finish();
}
