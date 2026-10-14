//! The observed Designer 2.20 attribute default tail extends the minimal authored field profile.

use super::super::super::{Descriptor, Error, Property, PropertyName, PropertyValue, Section};

/// An incomplete, reordered or customized default tail is outside the verified creation profile.
pub(super) fn attribute(properties: &[Property]) -> Result<bool, Error> {
    let [
        Property::Typed {
            name: PropertyName::Type,
            value: PropertyValue::TypeDescription(_),
        },
        extra @ ..,
    ] = properties
    else {
        return Ok(false);
    };
    if extra.is_empty() {
        return Ok(true);
    }
    let descriptor = Descriptor::from_xml(&format!(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' xmlns:v8='http://v8.1c.ru/8.1/data/core' xmlns:xs='http://www.w3.org/2001/XMLSchema' version='2.20'><Attribute uuid='00000000-0000-4000-8000-000000000001'><Properties><Name>Defaults</Name><Synonym/><Comment/><Type><v8:Type>xs:boolean</v8:Type></Type>{XML}</Properties></Attribute></MetaDataObject>"
    ))?;
    let [Section::Properties(expected)] = descriptor.object.sections.as_slice() else {
        return Err(Error::root("source_child_creation_profile_unsupported"));
    };
    Ok(expected.get(4..).is_some_and(|expected| extra == expected))
}

/// These exact platform-filled slots contain no identities, references or new editable defaults.
pub(in crate::project_core::source_format::child_creation) const XML: &str = concat!(
    "<PasswordMode>false</PasswordMode><Format/><EditFormat/><ToolTip/>",
    "<MarkNegatives>false</MarkNegatives><Mask/><MultiLine>false</MultiLine>",
    "<ExtendedEdit>false</ExtendedEdit><MinValue xsi:nil=\"true\"/><MaxValue xsi:nil=\"true\"/>",
    "<FillFromFillingValue>false</FillFromFillingValue><FillValue xsi:nil=\"true\"/>",
    "<FillChecking>DontCheck</FillChecking><ChoiceFoldersAndItems>Items</ChoiceFoldersAndItems>",
    "<ChoiceParameterLinks/><ChoiceParameters/><QuickChoice>Auto</QuickChoice>",
    "<CreateOnInput>Auto</CreateOnInput><ChoiceForm/><LinkByType/>",
    "<ChoiceHistoryOnInput>Auto</ChoiceHistoryOnInput><Use>ForItem</Use>",
    "<Indexing>DontIndex</Indexing><FullTextSearch>Use</FullTextSearch><DataHistory>Use</DataHistory>"
);
