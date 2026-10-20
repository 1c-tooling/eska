use super::*;
const UUID: &str = "50791551-3395-4b3f-94e4-c4dac0be017f";

/// A tiny synthetic v6 sample keeps parser assertions independent of production dumps.
fn fixture(mode: u8, rule: u8) -> String {
    format!(
        "{{6,0,1,{UUID},{mode},{UUID},\"1.0\",\"Vendor, \"\"name\"\"\",\"Configuration\",1,{rule},0,{UUID},{UUID},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}"
    )
}
#[test]
fn policies_preserve_support_origin() {
    for (rule, state, reason) in [
        (0, State::Locked, Reason::VendorLocked),
        (1, State::EditableWithSupport, Reason::EditableWithSupport),
        (2, State::Unrestricted, Reason::SupportRemoved),
    ] {
        let support = Support::parse(&fixture(0, rule)).unwrap();
        assert_eq!(support.object(UUID), (state, reason));
        assert_eq!(
            support.object("missing"),
            (State::Unrestricted, Reason::OwnObject)
        );
        assert_eq!(support.suppliers[0].vendor, "Vendor, \"name\"");
    }
}
#[test]
fn supplier_lock_overrides_rules_and_absent_ids() {
    let support = Support::parse(&fixture(1, 2)).unwrap();
    assert_eq!(
        support.object(UUID),
        (State::Locked, Reason::ConfigurationLocked)
    );
    assert_eq!(
        support.object("missing"),
        (State::Locked, Reason::ConfigurationLocked)
    );
}
#[test]
fn malformed_data_never_becomes_unrestricted() {
    let valid = fixture(0, 1);
    for input in [
        String::new(),
        valid[..valid.len() - 3].to_owned(),
        fixture(0, 3),
        valid.replacen("{6", "{7", 1),
        valid.replace(UUID, "invalid"),
    ] {
        assert!(Support::parse(&input).is_err());
    }
    assert!(Support::parse(&format!("\u{feff}{}\r\n", valid.replace(',', ",\r\n"))).is_ok());
}
#[test]
fn multiple_suppliers_intersect_permissions() {
    let mut support = Support::parse(&fixture(0, 2)).unwrap();
    support
        .suppliers
        .extend(Support::parse(&fixture(0, 0)).unwrap().suppliers);
    assert_eq!(support.object(UUID), (State::Locked, Reason::VendorLocked));
}

/// Explicit opt-in audit reads a supplied real dump and never changes it.
#[test]
#[ignore = "requires ESKA_SUPPORT_SAMPLE pointing to a real Designer dump"]
fn reads_real_support_sample() {
    let path = std::env::var("ESKA_SUPPORT_SAMPLE").unwrap();
    let input = std::fs::read_to_string(path).unwrap();
    let support = Support::parse(&input).unwrap();
    assert!(!support.suppliers.is_empty());
    for supplier in support.suppliers {
        eprintln!(
            "vendor={} configuration={} rules={} locked={}",
            supplier.vendor,
            supplier.name,
            supplier.rules.len(),
            supplier.locked
        );
    }
}
