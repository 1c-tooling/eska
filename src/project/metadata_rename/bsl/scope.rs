//! Lexical scopes disqualify shadowed globals before a metadata access can authorize replacement.

use std::{collections::BTreeSet, ops::Range};

use super::{
    contains_name,
    lex::{Kind, Token},
};

#[derive(Default)]
pub(super) struct Scope {
    pub names: BTreeSet<String>,
    pub dynamic: bool,
}

pub(super) struct Scopes {
    pub module: Scope,
    routines: Vec<(Range<usize>, Scope)>,
}

impl Scopes {
    /// Parse declarations in all preprocessor branches, retaining conservative union semantics.
    pub fn read(tokens: &[&Token<'_>]) -> Option<Self> {
        let mut scopes = Self {
            module: Scope::default(),
            routines: Vec::new(),
        };
        let mut active: Option<(usize, bool, Scope)> = None;
        let mut index = 0;
        while index < tokens.len() {
            let token = tokens[index];
            let procedure = token.word(&["Procedure", "Процедура"]);
            if procedure || token.word(&["Function", "Функция"]) {
                if active.is_some() || !standalone(tokens, index) {
                    return None;
                }
                let name = tokens
                    .get(index + 1)
                    .filter(|token| token.kind == Kind::Word)?;
                scopes.module.names.insert(name.text.to_owned());
                let (end, parameters) = parameters(tokens, index + 2)?;
                active = Some((
                    index,
                    procedure,
                    Scope {
                        names: parameters,
                        dynamic: false,
                    },
                ));
                index = end + 1;
                continue;
            }
            if token.word(&[
                "EndProcedure",
                "КонецПроцедуры",
                "EndFunction",
                "КонецФункции",
            ]) {
                let (start, procedure, scope) = active.take()?;
                if !standalone(tokens, index)
                    || procedure != token.word(&["EndProcedure", "КонецПроцедуры"])
                {
                    return None;
                }
                scopes.routines.push((start..index + 1, scope));
            } else {
                let scope = active
                    .as_mut()
                    .map_or(&mut scopes.module, |(_, _, scope)| scope);
                if token.word(&["Var", "Перем"]) && standalone(tokens, index) {
                    index = variables(tokens, index + 1, &mut scope.names)?;
                } else if standalone(tokens, index) && token.kind == Kind::Word {
                    if tokens
                        .get(index + 1)
                        .is_some_and(|next| next.kind == Kind::Symbol('='))
                        || index > 0 && tokens[index - 1].word(&["Each", "Каждого"])
                    {
                        // A comparison may over-block a name, but must never authorize a wrong binding.
                        scope.names.insert(token.text.to_owned());
                    }
                    if token.word(&["Execute", "Выполнить", "Eval", "Вычислить"])
                    {
                        scope.dynamic = true;
                    }
                }
            }
            index += 1;
        }
        active.is_none().then_some(scopes)
    }

    /// Module variables/methods shadow globals in every routine; local parameters remain local.
    pub fn shadowed(&self, index: usize, name: &str) -> bool {
        self.module.dynamic
            || contains_name(&self.module.names, name)
            || self.routines.iter().any(|(range, scope)| {
                range.contains(&index) && (scope.dynamic || contains_name(&scope.names, name))
            })
    }
}

/// A member of another value is never a declaration or a bare global identifier.
pub(super) const fn standalone(tokens: &[&Token<'_>], index: usize) -> bool {
    index == 0 || !matches!(tokens[index - 1].kind, Kind::Symbol('.' | '~'))
}

/// Parameters can contain literal defaults, but executable expressions or malformed lists are not inferred.
fn parameters(tokens: &[&Token<'_>], mut index: usize) -> Option<(usize, BTreeSet<String>)> {
    if tokens.get(index)?.kind != Kind::Symbol('(') {
        return None;
    }
    index += 1;
    let mut names = BTreeSet::new();
    if tokens.get(index)?.kind == Kind::Symbol(')') {
        return Some((index, names));
    }
    loop {
        if tokens.get(index)?.word(&["Val", "Знач"]) {
            index += 1;
        }
        let name = tokens.get(index).filter(|token| token.kind == Kind::Word)?;
        if !names.insert(name.text.to_owned()) {
            return None;
        }
        index += 1;
        if tokens.get(index)?.kind == Kind::Symbol('=') {
            index += 1;
            if tokens.get(index)?.kind == Kind::Symbol('-') {
                index += 1;
            }
            let value = tokens.get(index)?;
            if value.kind != Kind::Literal
                && !value.word(&[
                    "Undefined",
                    "Неопределено",
                    "True",
                    "Истина",
                    "False",
                    "Ложь",
                    "Null",
                ])
            {
                return None;
            }
            index += 1;
            // A decimal literal is lexed around its dot so it cannot be confused with a member access.
            if tokens.get(index)?.kind == Kind::Symbol('.') {
                index += 1;
                if tokens.get(index)?.kind != Kind::Literal {
                    return None;
                }
                index += 1;
            }
        }
        match tokens.get(index)?.kind {
            Kind::Symbol(')') => return Some((index, names)),
            Kind::Symbol(',') => index += 1,
            _ => return None,
        }
    }
}

/// Explicit variable lists cannot accidentally consume the first name of the next statement.
fn variables(
    tokens: &[&Token<'_>],
    mut index: usize,
    names: &mut BTreeSet<String>,
) -> Option<usize> {
    loop {
        let name = tokens.get(index).filter(|token| token.kind == Kind::Word)?;
        names.insert(name.text.to_owned());
        index += 1;
        if tokens.get(index)?.word(&["Export", "Экспорт"]) {
            index += 1;
        }
        match tokens.get(index)?.kind {
            Kind::Symbol(';') => return Some(index),
            Kind::Symbol(',') => index += 1,
            _ => return None,
        }
    }
}
