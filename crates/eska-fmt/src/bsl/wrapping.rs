//! Continuations split the lowest-precedence expression before considering its smaller operands.

use super::{expressions::precedence, layout, syntax::group_end};
use crate::lex::Token;
use eska_project_core::formatting::{Error, Options, Wrapping};
use std::ops::Range;

/// A plan distinguishes the owning assignment from comparisons inside calls and expressions.
pub(super) fn plan(
    input: &str,
    tokens: &[Token<'_>],
    options: &Options,
    initial: usize,
    base: usize,
    assignment: Option<usize>,
) -> Result<Vec<bool>, Error> {
    let mut planner = Planner {
        input,
        tokens,
        options,
        continuation: base + usize::from(options.continuation_indent_width),
        assignment,
        breaks: vec![false; tokens.len()],
    };
    planner.segment(0..tokens.len(), initial)?;
    Ok(planner.breaks)
}

struct Planner<'a, 'b> {
    input: &'a str,
    tokens: &'a [Token<'b>],
    options: &'a Options,
    continuation: usize,
    assignment: Option<usize>,
    breaks: Vec<bool>,
}

impl Planner<'_, '_> {
    /// Each recursive level consumes one precedence tier, bounding rescans to the grammar's tiers.
    fn segment(&mut self, range: Range<usize>, initial: usize) -> Result<(), Error> {
        let mut operators = Vec::new();
        let mut at = range.start;
        while at < range.end {
            if self.tokens[at].is("(") || self.tokens[at].is("[") {
                at = group_end(self.tokens, at)?;
                continue;
            }
            if self.assignment != Some(at)
                && !layout::unary(self.tokens, at)
                && let Some(priority) = precedence(&self.tokens[at])
            {
                operators.push((at, priority));
            }
            at += 1;
        }
        let Some(minimum) = operators.iter().map(|(_, priority)| *priority).min() else {
            return Ok(());
        };
        let long = self.options.column_limit != 0
            && layout::width(
                &layout::flat(&self.tokens[range.clone()], self.options),
                initial,
                self.options.tab_width,
            ) > usize::from(self.options.column_limit);
        let mut first = range.start;
        let mut column = initial;
        for (at, priority) in operators
            .into_iter()
            .filter(|(_, priority)| *priority == minimum)
        {
            let existing = self.existing_break(at);
            self.breaks[at] = match self.options.wrap_binary {
                Wrapping::OnePerLine => true,
                Wrapping::Preserve => existing,
                // A short comparison is atomic even if an earlier formatter split its equals sign.
                Wrapping::Fit => {
                    long || priority != 30 && self.options.preserve_existing_line_breaks && existing
                }
            };
            self.segment(first..at, column)?;
            column = if self.breaks[at] {
                self.continuation
            } else {
                layout::width(
                    &layout::flat(&self.tokens[first..at], self.options),
                    column,
                    self.options.tab_width,
                ) + usize::from(self.options.space_around_operators)
            };
            column = layout::width(self.tokens[at].text, column, self.options.tab_width)
                + usize::from(self.options.space_around_operators);
            first = at + 1;
        }
        self.segment(first..range.end, column)
    }

    /// Only trivia surrounding an operator can request a preserved continuation.
    fn existing_break(&self, at: usize) -> bool {
        at.checked_sub(1).is_some_and(|previous| {
            self.input[self.tokens[previous].range.end..self.tokens[at].range.start].contains('\n')
        }) || self.tokens.get(at + 1).is_some_and(|next| {
            self.input[self.tokens[at].range.end..next.range.start].contains('\n')
        })
    }
}
