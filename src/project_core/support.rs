//! Vendor support is independent of editor permissions and never writes source files.

use std::collections::BTreeMap;

/// Effective object policy; unrestricted objects retain their origin separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Locked,
    EditableWithSupport,
    Unrestricted,
    Unknown,
}

/// Machine-readable explanation, localized only by the consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    ConfigurationLocked,
    VendorLocked,
    EditableWithSupport,
    OwnObject,
    SupportRemoved,
    Unavailable,
}

/// Supplier identity and raw rules remain available for diagnostics.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Supplier {
    pub id: String,
    pub configuration_uuid: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub locked: bool,
    pub rules: BTreeMap<String, u8>,
}

/// A completely parsed file; malformed input cannot become an empty policy.
#[derive(Clone, Debug)]
pub struct Support {
    pub suppliers: Vec<Supplier>,
}

/// Explicit rejection with a token position, without disclosing source contents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub code: &'static str,
    pub position: usize,
}

impl Support {
    /// Parse the observed Designer format with bounded input and strict counts.
    ///
    /// # Errors
    /// Returns unsupported formats, invalid UUIDs, duplicate rules and truncated input.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let mut tokens = Tokens::new(input)?;
        if tokens.take()? != "6" {
            return Err(tokens.error("unsupported_version"));
        }
        // This field differs in real dumps with identical object rules. It is not
        // the supplier's modification mode and must not override that mode.
        tokens.flag()?;
        let count = tokens.number()?;
        if count > 64 {
            return Err(tokens.error("too_many_suppliers"));
        }
        let mut suppliers = Vec::new();
        for _ in 0..count {
            let id = tokens.uuid()?;
            let locked = tokens.flag()?;
            let configuration_uuid = tokens.uuid()?;
            let version = tokens.take()?;
            let vendor = tokens.take()?;
            let name = tokens.take()?;
            let size = tokens.number()?;
            if size > 1_000_000 {
                return Err(tokens.error("too_many_objects"));
            }
            let mut rules = BTreeMap::new();
            for _ in 0..size {
                let rule = u8::try_from(tokens.number()?)
                    .ok()
                    .filter(|rule| *rule <= 2)
                    .ok_or_else(|| tokens.error("invalid_rule"))?;
                tokens.flag()?;
                let uuid = tokens.uuid()?;
                tokens.uuid()?;
                if rules.insert(uuid, rule).is_some() {
                    return Err(tokens.error("duplicate_uuid"));
                }
            }
            // Nonempty per-supplier auxiliary sections need a separately verified layout.
            if tokens.number()? != 0 || tokens.number()? != 0 {
                return Err(tokens.error("unsupported_supplier_section"));
            }
            suppliers.push(Supplier {
                id,
                configuration_uuid,
                name,
                vendor,
                version,
                locked,
                rules,
            });
        }
        // Observed v6 suffix: three common fields and ten flags per supplier.
        for _ in 0..(3 + count * 10) {
            tokens.flag()?;
        }
        if tokens.next < tokens.values.len() {
            return Err(tokens.error("unexpected_trailing_data"));
        }
        if suppliers.len() > 1 && suppliers.iter().any(|supplier| supplier.locked) {
            return Err(tokens.error("unsupported_multi_supplier_lock"));
        }
        Ok(Self { suppliers })
    }

    /// Intersect supplier restrictions; an absent UUID is distinct from a removed rule.
    #[must_use]
    pub fn object(&self, uuid: &str) -> (State, Reason) {
        if self.suppliers.iter().any(|supplier| supplier.locked) {
            return (State::Locked, Reason::ConfigurationLocked);
        }
        let uuid = uuid.to_ascii_lowercase();
        let rules: Vec<_> = self
            .suppliers
            .iter()
            .filter_map(|supplier| supplier.rules.get(&uuid))
            .collect();
        if rules.contains(&&0) {
            (State::Locked, Reason::VendorLocked)
        } else if rules.contains(&&1) {
            (State::EditableWithSupport, Reason::EditableWithSupport)
        } else if rules.is_empty() {
            (State::Unrestricted, Reason::OwnObject)
        } else {
            (State::Unrestricted, Reason::SupportRemoved)
        }
    }
}

/// CSV-like 1C scalars inside a single envelope, including doubled quotes and CRLF.
struct Tokens {
    values: Vec<String>,
    next: usize,
}
impl Tokens {
    /// Tokenize without treating commas or newlines inside quoted names as separators.
    fn new(input: &str) -> Result<Self, ParseError> {
        let error = || ParseError {
            code: "invalid_envelope",
            position: 0,
        };
        if input.len() > 64 * 1024 * 1024 {
            return Err(error());
        }
        let input = input.trim_start_matches('\u{feff}').trim();
        let body = input
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or_else(error)?;
        let mut chars = body.chars().peekable();
        let mut values = Vec::new();
        loop {
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            let mut value = String::new();
            if chars.peek() == Some(&'"') {
                chars.next();
                loop {
                    match chars.next() {
                        Some('"') if chars.peek() == Some(&'"') => {
                            chars.next();
                            value.push('"');
                        }
                        Some('"') => break,
                        Some(c) => value.push(c),
                        None => return Err(error()),
                    }
                }
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            } else {
                while chars.peek().is_some_and(|c| *c != ',') {
                    let c = chars.next().ok_or_else(error)?;
                    if matches!(c, '"' | '{' | '}') {
                        return Err(error());
                    }
                    value.push(c);
                }
                let trimmed = value.trim().to_owned();
                value = trimmed;
                if value.is_empty() {
                    return Err(error());
                }
            }
            values.push(value);
            match chars.next() {
                None => break,
                Some(',') => (),
                _ => return Err(error()),
            }
        }
        Ok(Self { values, next: 0 })
    }
    /// Return only static diagnostic codes and an input position.
    const fn error(&self, code: &'static str) -> ParseError {
        ParseError {
            code,
            position: self.next,
        }
    }
    /// Consume one scalar, rejecting truncation explicitly.
    fn take(&mut self) -> Result<String, ParseError> {
        let value = self
            .values
            .get(self.next)
            .cloned()
            .ok_or_else(|| self.error("truncated"))?;
        self.next += 1;
        Ok(value)
    }
    /// Counts are bounded separately before any allocation or iteration.
    fn number(&mut self) -> Result<usize, ParseError> {
        self.take()?
            .parse()
            .map_err(|_| self.error("invalid_number"))
    }
    /// Unknown enum values must not silently enable editing.
    fn flag(&mut self) -> Result<bool, ParseError> {
        match self.take()?.as_str() {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err(self.error("invalid_flag")),
        }
    }
    /// UUID matching is case-insensitive but requires the canonical hyphenated shape.
    fn uuid(&mut self) -> Result<String, ParseError> {
        let value = self.take()?;
        if value.len() != 36
            || !value.bytes().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            })
        {
            return Err(self.error("invalid_uuid"));
        }
        Ok(value.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests;
