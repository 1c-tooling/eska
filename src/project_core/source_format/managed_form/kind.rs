//! Child domains distinguish pages, data columns and command containers without inferring type by suffix.

use serde::{Deserialize, Serialize};

/// All observed UI declaration classes have one identity, including owned menu/tooltip/addition slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum FormElementKind {
    AutoCommandBar,
    Button,
    ButtonGroup,
    CalendarField,
    ChartField,
    CheckBoxField,
    ColumnGroup,
    CommandBar,
    ContextMenu,
    ExtendedTooltip,
    FormattedDocumentField,
    #[serde(rename = "HTMLDocumentField")]
    HtmlDocumentField,
    InputField,
    LabelDecoration,
    LabelField,
    Page,
    Pages,
    PictureDecoration,
    PictureField,
    Popup,
    ProgressBarField,
    RadioButtonField,
    SearchControlAddition,
    SearchStringAddition,
    #[serde(rename = "SpreadSheetDocumentField")]
    SpreadsheetDocumentField,
    Table,
    TextDocumentField,
    UsualGroup,
    ViewStatusAddition,
}

impl FormElementKind {
    /// Native tag spelling is independent of the Rust identifier and remains exact for XML output.
    pub(in crate::project_core::source_format) const fn native_name(self) -> &'static str {
        match self {
            Self::AutoCommandBar => "AutoCommandBar",
            Self::Button => "Button",
            Self::ButtonGroup => "ButtonGroup",
            Self::CalendarField => "CalendarField",
            Self::ChartField => "ChartField",
            Self::CheckBoxField => "CheckBoxField",
            Self::ColumnGroup => "ColumnGroup",
            Self::CommandBar => "CommandBar",
            Self::ContextMenu => "ContextMenu",
            Self::ExtendedTooltip => "ExtendedTooltip",
            Self::FormattedDocumentField => "FormattedDocumentField",
            Self::HtmlDocumentField => "HTMLDocumentField",
            Self::InputField => "InputField",
            Self::LabelDecoration => "LabelDecoration",
            Self::LabelField => "LabelField",
            Self::Page => "Page",
            Self::Pages => "Pages",
            Self::PictureDecoration => "PictureDecoration",
            Self::PictureField => "PictureField",
            Self::Popup => "Popup",
            Self::ProgressBarField => "ProgressBarField",
            Self::RadioButtonField => "RadioButtonField",
            Self::SearchControlAddition => "SearchControlAddition",
            Self::SearchStringAddition => "SearchStringAddition",
            Self::SpreadsheetDocumentField => "SpreadSheetDocumentField",
            Self::Table => "Table",
            Self::TextDocumentField => "TextDocumentField",
            Self::UsualGroup => "UsualGroup",
            Self::ViewStatusAddition => "ViewStatusAddition",
        }
    }

    /// Exact native names are a closed profile; an unknown XML tag never inherits another class's layout rights.
    pub(super) fn native(tag: &str) -> Option<Self> {
        Some(match tag {
            "AutoCommandBar" => Self::AutoCommandBar,
            "Button" => Self::Button,
            "ButtonGroup" => Self::ButtonGroup,
            "CalendarField" => Self::CalendarField,
            "ChartField" => Self::ChartField,
            "CheckBoxField" => Self::CheckBoxField,
            "ColumnGroup" => Self::ColumnGroup,
            "CommandBar" => Self::CommandBar,
            "ContextMenu" => Self::ContextMenu,
            "ExtendedTooltip" => Self::ExtendedTooltip,
            "FormattedDocumentField" => Self::FormattedDocumentField,
            "HTMLDocumentField" => Self::HtmlDocumentField,
            "InputField" => Self::InputField,
            "LabelDecoration" => Self::LabelDecoration,
            "LabelField" => Self::LabelField,
            "Page" => Self::Page,
            "Pages" => Self::Pages,
            "PictureDecoration" => Self::PictureDecoration,
            "PictureField" => Self::PictureField,
            "Popup" => Self::Popup,
            "ProgressBarField" => Self::ProgressBarField,
            "RadioButtonField" => Self::RadioButtonField,
            "SearchControlAddition" => Self::SearchControlAddition,
            "SearchStringAddition" => Self::SearchStringAddition,
            "SpreadSheetDocumentField" => Self::SpreadsheetDocumentField,
            "Table" => Self::Table,
            "TextDocumentField" => Self::TextDocumentField,
            "UsualGroup" => Self::UsualGroup,
            "ViewStatusAddition" => Self::ViewStatusAddition,
            _ => return None,
        })
    }

    /// A regular item has a concrete owner/role; fixed menus and tooltips cannot become ordinary children.
    pub(super) const fn regular(self) -> bool {
        !matches!(
            self,
            Self::AutoCommandBar
                | Self::ContextMenu
                | Self::ExtendedTooltip
                | Self::ViewStatusAddition
        )
    }

    /// Ordered collections exist only for proven group, page, table and command containers.
    pub(super) const fn container(self) -> bool {
        matches!(
            self,
            Self::AutoCommandBar
                | Self::ButtonGroup
                | Self::ColumnGroup
                | Self::CommandBar
                | Self::ContextMenu
                | Self::Page
                | Self::Pages
                | Self::Popup
                | Self::Table
                | Self::UsualGroup
        )
    }

    /// Ordinary visual groups exclude page/column/command-only members with a different native parent domain.
    pub(super) const fn visual(self) -> bool {
        self.regular()
            && !matches!(
                self,
                Self::Page | Self::ColumnGroup | Self::ButtonGroup | Self::Popup
            )
    }

    /// Data-column and command containers have stricter native child domains than general visual groups.
    pub(super) const fn accepts(self, child: Self) -> bool {
        match self {
            Self::Pages => matches!(child, Self::Page),
            Self::Table | Self::ColumnGroup => matches!(
                child,
                Self::ColumnGroup
                    | Self::InputField
                    | Self::LabelField
                    | Self::CheckBoxField
                    | Self::PictureField
            ),
            Self::ButtonGroup | Self::ContextMenu | Self::Popup => {
                matches!(child, Self::Button | Self::ButtonGroup | Self::Popup)
            }
            Self::AutoCommandBar | Self::CommandBar => matches!(
                child,
                Self::Button
                    | Self::ButtonGroup
                    | Self::Popup
                    | Self::SearchStringAddition
                    | Self::SearchControlAddition
            ),
            Self::Page | Self::UsualGroup => child.visual(),
            _ => false,
        }
    }

    /// Only a declared fixed slot may use one of these parent relationships.
    pub(super) const fn fixed_owner(self, owner: Option<Self>) -> bool {
        match self {
            Self::AutoCommandBar => matches!(owner, None | Some(Self::Table)),
            Self::SearchStringAddition | Self::SearchControlAddition | Self::ViewStatusAddition => {
                matches!(owner, Some(Self::Table))
            }
            Self::ContextMenu => matches!(
                owner,
                Some(
                    Self::InputField
                        | Self::LabelField
                        | Self::CheckBoxField
                        | Self::RadioButtonField
                        | Self::PictureField
                        | Self::CalendarField
                        | Self::ChartField
                        | Self::FormattedDocumentField
                        | Self::HtmlDocumentField
                        | Self::ProgressBarField
                        | Self::SpreadsheetDocumentField
                        | Self::TextDocumentField
                        | Self::LabelDecoration
                        | Self::PictureDecoration
                        | Self::Table
                        | Self::SearchStringAddition
                        | Self::SearchControlAddition
                        | Self::ViewStatusAddition
                )
            ),
            Self::ExtendedTooltip => {
                matches!(owner, Some(kind) if kind.regular() || matches!(kind, Self::ViewStatusAddition))
            }
            _ => false,
        }
    }
}
