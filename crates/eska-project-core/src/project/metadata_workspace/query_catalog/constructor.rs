//! Derived-source columns are selected from their own first UNION branch, with metadata-backed wildcard expansion.

use super::{
    ProjectSession, QueryCatalogPage, QueryCatalogRequest, QueryFieldChoice, QuerySessionError,
};
use crate::project::query::{
    QueryError,
    constructor::{ClauseKind, Constructor, Selection},
};
use std::collections::BTreeMap;

impl ProjectSession {
    /// Resolve a constructor-bound source without asking an IDE to parse its table expression.
    ///
    /// # Errors
    /// Rejects stale selections, excessive nesting and unreadable application metadata.
    pub fn query_constructor_catalog(
        &self,
        constructor: &Constructor,
        application: Option<&str>,
        request: &QueryCatalogRequest,
    ) -> Result<QueryCatalogPage, QuerySessionError> {
        request.validate()?;
        self.constructor_catalog_source(constructor, application, request, 0)
    }

    /// Acyclic source scopes are still bounded before recursively expanding nested wildcards.
    fn constructor_catalog_source(
        &self,
        constructor: &Constructor,
        application: Option<&str>,
        request: &QueryCatalogRequest,
        depth: usize,
    ) -> Result<QueryCatalogPage, QuerySessionError> {
        if depth > 32 {
            return Err(QueryError::TooLarge.into());
        }
        if let Some(scope) = request.scope {
            return self.constructor_catalog_projection(
                constructor,
                application,
                request,
                scope,
                depth,
            );
        }
        let Some(selection) = request.source else {
            return self.query_catalog(application, request);
        };
        let model = constructor.model();
        let owner = model
            .statements
            .get(selection.statement)
            .ok_or(QueryError::InvalidPosition)?;
        let binding = owner
            .source_bindings(constructor.text())
            .into_iter()
            .find(|binding| binding.clause == selection.clause && binding.item == selection.item)
            .ok_or(QueryError::InvalidPosition)?;
        if !binding.derived {
            return self.query_catalog(
                application,
                &QueryCatalogRequest {
                    virtual_tables: request.virtual_tables || binding.parameters.is_some(),
                    table: Some(binding.table),
                    source: None,
                    ..request.clone()
                },
            );
        }
        let scope = model
            .source_scope(selection)
            .ok_or(QueryError::InvalidPosition)?;
        self.constructor_catalog_projection(constructor, application, request, scope, depth)
    }

    /// Root and nested results share the same ordinal projection and wildcard expansion rules.
    fn constructor_catalog_projection(
        &self,
        constructor: &Constructor,
        application: Option<&str>,
        request: &QueryCatalogRequest,
        scope: usize,
        depth: usize,
    ) -> Result<QueryCatalogPage, QuerySessionError> {
        let model = constructor.model();
        let selected = model.scopes.get(scope).ok_or(QueryError::InvalidPosition)?;
        let branch = *selected
            .branches
            .first()
            .ok_or(QueryError::InvalidPosition)?;
        let statement = &model.statements[branch];
        let mut fields = BTreeMap::new();
        if let Some(clause) = statement
            .clauses
            .iter()
            .find(|clause| clause.kind == ClauseKind::Fields)
        {
            for item in &clause.items {
                if let Some(name) = item.output_name(constructor.text(), clause) {
                    fields.entry(name.to_lowercase()).or_insert(name);
                } else if let Some(source) = item.wildcard_source(constructor.text(), clause) {
                    for binding in statement
                        .source_bindings(constructor.text())
                        .into_iter()
                        .filter(|binding| {
                            source.as_ref().is_none_or(|alias| {
                                alias.to_lowercase() == binding.alias.to_lowercase()
                            })
                        })
                    {
                        let page = self.constructor_catalog_source(
                            constructor,
                            application,
                            &QueryCatalogRequest {
                                virtual_tables: true,
                                source: Some(Selection {
                                    statement: branch,
                                    clause: binding.clause,
                                    item: binding.item,
                                }),
                                ..Default::default()
                            },
                            depth + 1,
                        )?;
                        for field in page.fields {
                            let name = if statement.russian {
                                field.russian
                            } else {
                                field.english
                            };
                            fields.entry(name.to_lowercase()).or_insert(name);
                        }
                    }
                }
            }
        }
        if fields.len() > 65_536 {
            return Err(QueryError::TooLarge.into());
        }
        self.derived_page(application, request, fields)
    }

    /// Derived identifiers use their proven projection spelling in either editor locale.
    fn derived_page(
        &self,
        application: Option<&str>,
        request: &QueryCatalogRequest,
        fields: BTreeMap<String, String>,
    ) -> Result<QueryCatalogPage, QuerySessionError> {
        let context = self.query_context(application)?;
        let qualifier = request.qualifier.as_deref().unwrap_or("");
        let fields = fields
            .into_values()
            .map(|name| {
                let name = if qualifier.is_empty() {
                    name
                } else {
                    format!("{qualifier}.{name}")
                };
                QueryFieldChoice {
                    russian: name.clone(),
                    english: name,
                }
            })
            .collect();
        Ok(QueryCatalogPage {
            tables: Vec::new(),
            fields,
            next_offset: None,
            found: true,
            context_fingerprint: context.fingerprint,
            limited_context: context.context.limited_context(),
            parameters: Vec::new(),
        })
    }
}
