//! Proven global access chains use ESKA's own lexical scopes and reviewed platform aliases.

mod lex;
mod scope;

use std::collections::BTreeSet;

use super::{RenameReplacement, UncertainReference, XmlAnalysis, same_name};
use lex::{Kind, Token};
use scope::Scopes;

/// Project discovery must prove the visible global symbols before this resolver can change code.
#[derive(Default)]
pub struct GlobalBindings {
    pub complete: bool,
    pub shadowed: BTreeSet<String>,
    pub common_modules: BTreeSet<String>,
}

/// Object ancestry supplies semantic identity; filenames and free text cannot create a binding.
pub struct BslRename {
    ancestry: Vec<(String, String)>,
    old_name: String,
    new_name: String,
}

impl BslRename {
    /// Retain the resolved ancestry alongside XML reference analysis for the same immutable plan.
    pub fn new(ancestry: &[(String, String)], new_name: &str) -> Self {
        Self {
            ancestry: ancestry.to_vec(),
            old_name: ancestry
                .last()
                .map_or_else(String::new, |(_, name)| name.clone()),
            new_name: new_name.to_owned(),
        }
    }

    /// Replace only direct, unshadowed global accesses in modules whose implicit context is known.
    pub fn analyze(
        &self,
        input: &str,
        globals: &GlobalBindings,
        module_known: bool,
    ) -> XmlAnalysis {
        let Some(tokens) = lex::tokenize(input) else {
            return self.unverified(input, "bsl_lexical_unavailable");
        };
        let code: Vec<_> = tokens
            .iter()
            .filter(|token| token.kind != Kind::Comment)
            .collect();
        let Some(scopes) = Scopes::read(&code) else {
            return self.unverified(input, "bsl_scope_unavailable");
        };
        let mut result = XmlAnalysis::default();
        for (index, token) in code.iter().enumerate() {
            if token.kind != Kind::Word || !same_name(token.text, &self.old_name) {
                continue;
            }
            let root = self.binding_root(&code, index);
            let reason = match root {
                None => Some("bsl_binding_unverified"),
                Some(_) if !module_known => Some("bsl_module_context_unverified"),
                Some(_) if !globals.complete => Some("bsl_global_context_unverified"),
                Some(root)
                    if contains_name(&globals.shadowed, code[root].text)
                        || scopes.shadowed(root, code[root].text)
                        || self.ancestry[0].0 != "CommonModule"
                            && contains_name(&globals.common_modules, code[root].text) =>
                {
                    Some("bsl_shadowed_name")
                }
                Some(_)
                    if self.ancestry[0].0 == "CommonModule"
                        && !contains_name(&globals.common_modules, &self.old_name) =>
                {
                    Some("bsl_binding_unverified")
                }
                Some(root)
                    if self.ancestry[0].0 == "CommonModule"
                        && (contains_name(&globals.shadowed, &self.new_name)
                            || scopes.shadowed(root, &self.new_name)) =>
                {
                    Some("bsl_destination_shadowed")
                }
                Some(_) => None,
            };
            if let Some(reason) = reason {
                result.uncertain.push(UncertainReference {
                    range: token.range.clone(),
                    text: token.text.to_owned(),
                    reason,
                });
            } else {
                result.replacements.push(RenameReplacement {
                    range: token.range.clone(),
                    before: token.text.to_owned(),
                    after: self.new_name.clone(),
                });
            }
        }
        for token in &tokens {
            let reason = match token.kind {
                Kind::Comment => "bsl_comment",
                Kind::Literal => "bsl_literal",
                Kind::Directive => "bsl_directive",
                _ => continue,
            };
            result.uncertain.extend(occurrences(
                token.text,
                &self.old_name,
                token.range.start,
                reason,
            ));
        }
        result.uncertain.sort_by_key(|item| item.range.start);
        result
    }

    /// Recognize a manager/metadata prefix, never a similarly named member of an arbitrary receiver.
    fn binding_root(&self, code: &[&Token<'_>], index: usize) -> Option<usize> {
        let (class, name) = self.ancestry.first()?;
        if class == "CommonModule" && self.ancestry.len() == 1 {
            return (scope::standalone(code, index)
                && code.get(index + 1)?.kind == Kind::Symbol('.')
                && code.get(index + 2)?.kind == Kind::Word
                && code.get(index + 3)?.kind == Kind::Symbol('('))
            .then_some(index);
        }
        // A collection method such as Metadata.Catalogs.Count() is not a metadata object's property.
        if code
            .get(index + 1)
            .is_some_and(|token| token.kind == Kind::Symbol('('))
        {
            return None;
        }
        let aliases = manager_aliases(class)?;
        let (root, valid) = if self.ancestry.len() == 1 {
            let root = index.checked_sub(2)?;
            (
                root,
                code[root].word(&aliases) && code[root + 1].kind == Kind::Symbol('.'),
            )
        } else if self.ancestry.len() == 2 && class == "Enum" && self.ancestry[1].0 == "EnumValue" {
            let root = index.checked_sub(4)?;
            (
                root,
                code[root].word(&aliases)
                    && code[root + 1].kind == Kind::Symbol('.')
                    && code[root + 2].word(&[name])
                    && code[root + 3].kind == Kind::Symbol('.'),
            )
        } else {
            return None;
        };
        if !valid {
            return None;
        }
        if scope::standalone(code, root) {
            return Some(root);
        }
        if self.ancestry.len() != 1 {
            return None;
        }
        let metadata = root.checked_sub(2)?;
        (code[metadata].word(&["Metadata", "Метаданные"])
            && code[metadata + 1].kind == Kind::Symbol('.')
            && scope::standalone(code, metadata))
        .then_some(metadata)
    }

    /// Incomplete source remains reviewable without authorizing any replacement.
    fn unverified(&self, input: &str, reason: &'static str) -> XmlAnalysis {
        XmlAnalysis {
            replacements: Vec::new(),
            uncertain: occurrences(input, &self.old_name, 0, reason),
        }
    }
}

/// Module declarations can contribute global symbols; a malformed module cannot prove their absence.
pub fn module_names(input: &str) -> Option<BTreeSet<String>> {
    let tokens = lex::tokenize(input)?;
    let code: Vec<_> = tokens
        .iter()
        .filter(|token| token.kind != Kind::Comment)
        .collect();
    let module = Scopes::read(&code)?.module;
    (!module.dynamic).then_some(module.names)
}

/// Names in comments/strings keep exact offsets and are never promoted to executable references.
fn occurrences(
    input: &str,
    name: &str,
    base: usize,
    reason: &'static str,
) -> Vec<UncertainReference> {
    let mut output = Vec::new();
    let mut offset = base;
    for token in input.split_inclusive(|character| !lex::identifier_part(character)) {
        let word = token.trim_end_matches(|character| !lex::identifier_part(character));
        if same_name(word, name) {
            output.push(UncertainReference {
                range: offset..offset + word.len(),
                text: word.to_owned(),
                reason,
            });
        }
        offset += token.len();
    }
    output
}

/// Aliases are verified against 8.3.27 platform global-context cards, not translated UI labels.
fn manager_aliases(class: &str) -> Option<[&'static str; 2]> {
    Some(match class {
        "Catalog" => ["Catalogs", "Справочники"],
        "Document" => ["Documents", "Документы"],
        "Enum" => ["Enums", "Перечисления"],
        "Constant" => ["Constants", "Константы"],
        "ExchangePlan" => ["ExchangePlans", "ПланыОбмена"],
        "ChartOfAccounts" => ["ChartsOfAccounts", "ПланыСчетов"],
        "ChartOfCharacteristicTypes" => ["ChartsOfCharacteristicTypes", "ПланыВидовХарактеристик"],
        "ChartOfCalculationTypes" => ["ChartsOfCalculationTypes", "ПланыВидовРасчета"],
        "BusinessProcess" => ["BusinessProcesses", "БизнесПроцессы"],
        "Task" => ["Tasks", "Задачи"],
        "InformationRegister" => ["InformationRegisters", "РегистрыСведений"],
        "AccumulationRegister" => ["AccumulationRegisters", "РегистрыНакопления"],
        "AccountingRegister" => ["AccountingRegisters", "РегистрыБухгалтерии"],
        "CalculationRegister" => ["CalculationRegisters", "РегистрыРасчета"],
        "DataProcessor" => ["DataProcessors", "Обработки"],
        "Report" => ["Reports", "Отчеты"],
        "DocumentJournal" => ["DocumentJournals", "ЖурналыДокументов"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests;

/// Retain Unicode case equivalence without assuming that lowercase has one canonical spelling.
pub fn contains_name(names: &BTreeSet<String>, name: &str) -> bool {
    names.contains(name) || names.iter().any(|item| same_name(item, name))
}
