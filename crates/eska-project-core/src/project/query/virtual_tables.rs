//! Query-language virtual table signatures are shared by metadata choices and every constructor adapter.

use super::constructor::{Error, lex};
use crate::project::metadata_model::MetadataKind;

/// These identifiers describe query syntax, independently of the IDE language.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualTable {
    SliceFirst,
    SliceLast,
    Balance,
    Turnovers,
    BalanceAndTurnovers,
    RecordsWithExtDimensions,
    DrCrTurnovers,
    ExtDimensions,
    AdjustedEffectivePeriod,
    ScheduleData,
    Base,
}

/// Parameter keys belong to the protocol; their human labels belong to the presentation layer.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub key: &'static str,
    pub required: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<KeywordChoice>,
}

/// Grammar values are provided by ESKA in both query spellings, without asking a renderer to generate keywords.
#[derive(Clone, Debug, serde::Serialize)]
pub struct KeywordChoice {
    pub russian: &'static str,
    pub english: &'static str,
}

impl VirtualTable {
    /// Return the real bilingual query name rather than a localized UI caption.
    #[must_use]
    pub const fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::SliceFirst => ("СрезПервых", "SliceFirst"),
            Self::SliceLast => ("СрезПоследних", "SliceLast"),
            Self::Balance => ("Остатки", "Balance"),
            Self::Turnovers => ("Обороты", "Turnovers"),
            Self::BalanceAndTurnovers => ("ОстаткиИОбороты", "BalanceAndTurnovers"),
            Self::RecordsWithExtDimensions => ("ДвиженияССубконто", "RecordsWithExtDimensions"),
            Self::DrCrTurnovers => ("ОборотыДтКт", "DrCrTurnovers"),
            Self::ExtDimensions => ("Субконто", "ExtDimensions"),
            Self::AdjustedEffectivePeriod => {
                ("ФактическийПериодДействия", "AdjustedEffectivePeriod")
            }
            Self::ScheduleData => ("ДанныеГрафика", "ScheduleData"),
            Self::Base => ("База", "Base"),
        }
    }

    /// Match only the virtual suffix in one of the four register namespaces.
    #[must_use]
    pub fn resolve(table: &str) -> Option<(MetadataKind, Self)> {
        let mut path = table.split('.');
        let category = match path.next()?.to_lowercase().as_str() {
            "informationregister" | "регистрсведений" => {
                MetadataKind::InformationRegister
            }
            "accumulationregister" | "регистрнакопления" => {
                MetadataKind::AccumulationRegister
            }
            "accountingregister" | "регистрбухгалтерии" => {
                MetadataKind::AccountingRegister
            }
            "calculationregister" | "регистррасчета" => {
                MetadataKind::CalculationRegister
            }
            _ => return None,
        };
        if path.next()?.is_empty() {
            return None;
        }
        let suffix = path.next()?;
        if path.next().is_some() {
            return None;
        }
        Self::for_category(category)
            .iter()
            .copied()
            .find(|value| {
                let (russian, english) = value.names();
                suffix.eq_ignore_ascii_case(english)
                    || suffix.to_lowercase() == russian.to_lowercase()
                    || (*value == Self::Base
                        && (suffix.starts_with(russian) || suffix.starts_with(english)))
            })
            .map(|value| (category, value))
    }

    /// Metadata availability further narrows this namespace-level set.
    #[must_use]
    pub const fn for_category(category: MetadataKind) -> &'static [Self] {
        match category {
            MetadataKind::InformationRegister => &[Self::SliceFirst, Self::SliceLast],
            MetadataKind::AccumulationRegister => {
                &[Self::Balance, Self::Turnovers, Self::BalanceAndTurnovers]
            }
            MetadataKind::AccountingRegister => &[
                Self::Balance,
                Self::Turnovers,
                Self::BalanceAndTurnovers,
                Self::RecordsWithExtDimensions,
                Self::DrCrTurnovers,
                Self::ExtDimensions,
            ],
            MetadataKind::CalculationRegister => &[
                Self::AdjustedEffectivePeriod,
                Self::ScheduleData,
                Self::Base,
            ],
            _ => &[],
        }
    }

    /// Positional order follows the installed 8.3.27 query reference, including empty optional slots.
    #[must_use]
    pub fn parameters(self, category: MetadataKind, correspondence: bool) -> Vec<Parameter> {
        let keys: &[&str] = match (self, category) {
            (Self::Balance, MetadataKind::AccountingRegister) => {
                &["period", "accountCondition", "extraDimensions", "condition"]
            }
            (Self::SliceFirst | Self::SliceLast | Self::Balance, _) => &["period", "condition"],
            (Self::Turnovers, MetadataKind::AccountingRegister) if correspondence => &[
                "beginPeriod",
                "endPeriod",
                "periodicity",
                "accountCondition",
                "extraDimensions",
                "condition",
                "correspondingAccountCondition",
                "correspondingExtraDimensions",
            ],
            (Self::Turnovers, MetadataKind::AccountingRegister) => &[
                "beginPeriod",
                "endPeriod",
                "periodicity",
                "accountCondition",
                "extraDimensions",
                "condition",
            ],
            (Self::Turnovers, _) => &["beginPeriod", "endPeriod", "periodicity", "condition"],
            (Self::BalanceAndTurnovers, MetadataKind::AccountingRegister) => &[
                "beginPeriod",
                "endPeriod",
                "periodicity",
                "complementMethod",
                "accountCondition",
                "extraDimensions",
                "condition",
            ],
            (Self::BalanceAndTurnovers, _) => &[
                "beginPeriod",
                "endPeriod",
                "periodicity",
                "complementMethod",
                "condition",
            ],
            (Self::RecordsWithExtDimensions, _) => {
                &["beginPeriod", "endPeriod", "condition", "order", "top"]
            }
            (Self::DrCrTurnovers, _) => &[
                "beginPeriod",
                "endPeriod",
                "periodicity",
                "debitAccountCondition",
                "debitExtraDimensions",
                "creditAccountCondition",
                "creditExtraDimensions",
                "condition",
            ],
            (Self::AdjustedEffectivePeriod | Self::ScheduleData, _) => &["condition"],
            (Self::Base, _) => &["mainDimensions", "baseDimensions", "slices", "condition"],
            (Self::ExtDimensions, _) => &[],
        };
        keys.iter()
            .enumerate()
            .map(|(index, &key)| Parameter {
                key,
                required: self == Self::Base && index < 2,
                choices: parameter_choices(key, category, self),
            })
            .collect()
    }
}

/// Periodicity differs from PERIODS grouping units and from the period-complement method.
fn parameter_choices(key: &str, category: MetadataKind, table: VirtualTable) -> Vec<KeywordChoice> {
    let mut values = Vec::new();
    if key == "periodicity" {
        values.extend([
            ("Период", "Period"),
            ("Запись", "Record"),
            ("Регистратор", "Recorder"),
        ]);
        if category == MetadataKind::AccountingRegister && table != VirtualTable::DrCrTurnovers {
            values.push(("УточнениеПериода", "PeriodAdjustment"));
        }
        values.extend(
            super::constructor::PeriodUnit::ALL
                .iter()
                .map(|unit| unit.names()),
        );
        values.push(("Авто", "Auto"));
    } else if key == "complementMethod" {
        values.extend([
            ("Движения", "RegisterRecords"),
            (
                "ДвиженияИГраницыПериода",
                "RegisterRecordsAndPeriodBoundaries",
            ),
        ]);
    }
    values
        .into_iter()
        .map(|(russian, english)| KeywordChoice { russian, english })
        .collect()
}

/// Format positional values without letting one input escape into another argument or statement.
pub(super) fn call(table: &str, parameters: &[String]) -> Result<String, Error> {
    let (category, kind) = VirtualTable::resolve(table).ok_or(Error::InvalidExpression)?;
    let signature = kind.parameters(category, true);
    if parameters.len() > signature.len() {
        return Err(Error::InvalidExpression);
    }
    for (index, parameter) in signature.iter().enumerate() {
        let text = parameters.get(index).map_or("", String::as_str);
        if parameter.required && text.trim().is_empty() {
            return Err(Error::InvalidExpression);
        }
        if text.len() > 16_384 {
            return Err(Error::TooLarge);
        }
        validate_argument(text)?;
    }
    Ok(format!("{table}({})", parameters.join(", ")))
}

/// One positional operand cannot hide a closing bracket, another slot or a package separator.
pub(super) fn validate_argument(text: &str) -> Result<(), Error> {
    if text.len() > 16_384 {
        return Err(Error::TooLarge);
    }
    if lex::tokens(text)?.iter().any(|token| {
        matches!(
            token.kind,
            lex::Kind::Comment | lex::Kind::Symbol(';' | '{' | '}' | '\'' | '`')
        ) || (token.depth == 0 && token.kind == lex::Kind::Symbol(','))
    }) {
        return Err(Error::InvalidExpression);
    }
    Ok(())
}

/// Losslessly split an existing call for ordinary parameter controls; comments require exact preservation.
pub(crate) fn split_call(table: &str) -> Option<(&str, Vec<String>)> {
    if VirtualTable::resolve(table).is_some() {
        return Some((table, Vec::new()));
    }
    let (name, parameters) = split_arguments(table)?;
    VirtualTable::resolve(name)?;
    Some((name, parameters))
}

/// Split argument positions with the shared lexer, including empty slots and nested calls.
pub(super) fn split_arguments(table: &str) -> Option<(&str, Vec<String>)> {
    let tokens = lex::tokens(table).ok()?;
    let open = tokens
        .iter()
        .position(|token| token.kind == lex::Kind::Symbol('(') && token.depth == 0)?;
    let close = tokens.last()?;
    if close.kind != lex::Kind::Symbol(')') || close.depth != 0 {
        return None;
    }
    let name = table[..tokens[open].range.start].trim();
    let mut parameters = Vec::new();
    let mut start = tokens[open].range.end;
    for end in tokens[open + 1..]
        .iter()
        .filter(|token| token.depth == 1 && token.kind == lex::Kind::Symbol(','))
        .map(|token| token.range.start)
        .chain([close.range.start])
    {
        parameters.push(table[start..end].trim().to_owned());
        start = end + 1;
    }
    if parameters.len() == 1 && parameters[0].is_empty() {
        parameters.clear();
    }
    Some((name, parameters))
}
