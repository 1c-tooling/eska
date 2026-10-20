//! Validation and adoption of existing common-module descriptors.

use super::{MD, PatchError};

/// Preserve descriptor bytes except for the UUID and explicit adoption properties.
pub(super) fn adopt(xml: &str, name: &str, head: &str) -> Result<String, PatchError> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|e| PatchError::new("descriptor", e.to_string()))?;
    let root = document.root_element();
    let children: Vec<_> = root
        .children()
        .filter(roxmltree::Node::is_element)
        .collect();
    let [module] = children.as_slice() else {
        return Err(PatchError::new("descriptor", name));
    };
    if !root.has_tag_name((MD, "MetaDataObject")) || !module.has_tag_name((MD, "CommonModule")) {
        return Err(PatchError::new("descriptor", name));
    }
    let properties = module
        .children()
        .find(|node| node.has_tag_name((MD, "Properties")))
        .ok_or_else(|| PatchError::new("descriptor", name))?;
    let property = |key| {
        properties
            .children()
            .find(|node| node.has_tag_name((MD, key)))
            .and_then(|node| node.text())
    };
    for (key, value) in [
        ("Name", name),
        ("Global", "false"),
        ("Server", "true"),
        ("ClientManagedApplication", "false"),
        ("ClientOrdinaryApplication", "false"),
        ("Privileged", "false"),
        ("ReturnValuesReuse", "DontUse"),
    ] {
        if property(key) != Some(value) {
            return Err(PatchError::new("descriptor", format!("{name}.{key}")));
        }
    }
    let uuid = module
        .attribute_node("uuid")
        .ok_or_else(|| PatchError::new("descriptor", name))?;
    let original = uuid.value();
    if original.len() != 36
        || !original
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
    {
        return Err(PatchError::new("descriptor", name));
    }
    let hash = gix::objs::compute_hash(
        gix::hash::Kind::Sha1,
        gix::objs::Kind::Blob,
        format!("{original}:{head}").as_bytes(),
    )
    .map_err(|e| PatchError::new("descriptor", e.to_string()))?
    .to_string();
    let generated = format!(
        "{}-{}-4{}-a{}-{}",
        &hash[..8],
        &hash[8..12],
        &hash[13..16],
        &hash[17..20],
        &hash[20..32]
    );
    let start = properties.range().start;
    let insertion = start
        + xml[start..]
            .find('>')
            .ok_or_else(|| PatchError::new("descriptor", name))?
        + 1;
    let mut result = xml.to_owned();
    result.insert_str(insertion, &format!("<ObjectBelonging xmlns=\"{MD}\">Adopted</ObjectBelonging><ExtendedConfigurationObject xmlns=\"{MD}\">{original}</ExtendedConfigurationObject>"));
    result.replace_range(uuid.range_value(), &generated);
    Ok(result)
}
