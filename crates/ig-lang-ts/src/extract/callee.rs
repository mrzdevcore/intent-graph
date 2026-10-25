//! Callees: what an identifier, member or type name refers to, and the export tables.

use super::*;

impl<'s, 'a> Extractor<'s, 'a> {
    pub(super) fn ident_callee(&self, id: &IdentifierReference) -> Pending {
        let name = id.name.as_str().to_string();
        match id
            .reference_id
            .get()
            .and_then(|r| self.scoping.get_reference(r).symbol_id())
        {
            Some(s) => Pending::Sym(s, name),
            None => Pending::Ready(Callee::Free(name)),
        }
    }

    /// The declared or constructed type of `x` or `this.f`, if known.
    pub(super) fn type_of(&self, e: &Expression) -> Option<Pending> {
        match e {
            Expression::Identifier(id) => {
                let s = id
                    .reference_id
                    .get()
                    .and_then(|r| self.scoping.get_reference(r).symbol_id())?;
                self.typed.get(&s).cloned()
            }
            Expression::StaticMemberExpression(m)
                if matches!(
                    m.object.get_inner_expression(),
                    Expression::ThisExpression(_)
                ) =>
            {
                let class = self.this_ctx.last()?.as_ref()?;
                self.fields
                    .get(class)?
                    .get(m.property.name.as_str())
                    .cloned()
            }
            _ => None,
        }
    }

    pub(super) fn annotation_type(&self, ta: Option<&TSTypeAnnotation>) -> Option<Pending> {
        match &ta?.type_annotation {
            TSType::TSTypeReference(r) => self.type_name_callee(&r.type_name),
            _ => None,
        }
    }

    pub(super) fn new_type(&self, e: Option<&Expression>) -> Option<Pending> {
        match e?.get_inner_expression() {
            Expression::NewExpression(n) => self.callee_of(&n.callee),
            _ => None,
        }
    }

    /// Field types of a class: annotated or `new`-initialised properties, and constructor
    /// parameter properties (`private readonly repo: Repo`).
    pub(super) fn collect_fields(&mut self, class: &Class<'a>, id: &str) {
        let mut f = HashMap::new();
        for el in &class.body.body {
            match el {
                ClassElement::PropertyDefinition(p) => {
                    if let (Some(k), Some(t)) = (
                        p.key.static_name(),
                        self.annotation_type(p.type_annotation.as_deref())
                            .or_else(|| self.new_type(p.value.as_ref())),
                    ) {
                        f.insert(k.to_string(), t);
                    }
                }
                ClassElement::MethodDefinition(m)
                    if m.kind == MethodDefinitionKind::Constructor =>
                {
                    for p in &m.value.params.items {
                        if p.accessibility.is_none() && !p.readonly {
                            continue;
                        }
                        if let (BindingPattern::BindingIdentifier(b), Some(t)) = (
                            &p.pattern,
                            self.annotation_type(p.type_annotation.as_deref()),
                        ) {
                            f.insert(b.name.as_str().to_string(), t);
                        }
                    }
                }
                _ => {}
            }
        }
        if !f.is_empty() {
            self.fields.insert(id.to_string(), f);
        }
    }

    /// The import binding `id` refers to, if any.
    pub(super) fn import_of(&self, id: &IdentifierReference) -> Option<(String, ImportName)> {
        let s = id
            .reference_id
            .get()
            .and_then(|r| self.scoping.get_reference(r).symbol_id())?;
        if self.scoping.symbol_scope_id(s) != self.scoping.root_scope_id()
            || !self.scoping.symbol_flags(s).is_import()
        {
            return None;
        }
        self.imports_map.get(id.name.as_str()).cloned()
    }

    pub(super) fn namespace_of(&self, id: &IdentifierReference) -> Option<String> {
        match self.import_of(id)? {
            (spec, ImportName::Namespace) => Some(spec),
            _ => None,
        }
    }

    pub(super) fn callee_of(&self, e: &Expression) -> Option<Pending> {
        match e.get_inner_expression() {
            Expression::Identifier(id) => Some(self.ident_callee(id)),
            Expression::StaticMemberExpression(m) => {
                let prop = m.property.name.as_str().to_string();
                Some(match m.object.get_inner_expression() {
                    Expression::ThisExpression(_) => match self.this_ctx.last() {
                        Some(Some(c)) => Pending::Ready(Callee::Member {
                            class: c.clone(),
                            name: prop,
                            skip_self: false,
                        }),
                        _ => Pending::Ready(Callee::Guess(prop)),
                    },
                    Expression::Super(_) => match self.this_ctx.last() {
                        Some(Some(c)) => Pending::Ready(Callee::Member {
                            class: c.clone(),
                            name: prop,
                            skip_self: true,
                        }),
                        _ => Pending::Ready(Callee::Guess(prop)),
                    },
                    Expression::Identifier(o) => match self.namespace_of(o) {
                        Some(spec) => {
                            Pending::Ready(Callee::Import(spec, ImportName::named(&prop)))
                        }
                        None => match self.import_of(o) {
                            Some((spec, n)) => Pending::Ready(Callee::ImportMember(spec, n, prop)),
                            None => match self.type_of(m.object.get_inner_expression()) {
                                Some(t) => Pending::Typed(Box::new(t), prop),
                                None => Pending::Ready(Callee::Guess(prop)),
                            },
                        },
                    },
                    obj => match self.type_of(obj) {
                        Some(t) => Pending::Typed(Box::new(t), prop),
                        None => Pending::Ready(Callee::Guess(prop)),
                    },
                })
            }
            _ => None,
        }
    }

    pub(super) fn type_name_callee(&self, t: &TSTypeName) -> Option<Pending> {
        match t {
            TSTypeName::IdentifierReference(id) => Some(self.ident_callee(id)),
            TSTypeName::QualifiedName(q) => {
                let prop = q.right.name.as_str().to_string();
                if let TSTypeName::IdentifierReference(o) = &q.left
                    && let Some(spec) = self.namespace_of(o)
                {
                    return Some(Pending::Ready(Callee::Import(
                        spec,
                        ImportName::named(&prop),
                    )));
                }
                Some(Pending::Ready(Callee::Guess(prop)))
            }
            _ => None,
        }
    }
}

pub(super) fn export_tables(mr: &ModuleRecord, x: &Extractor) -> (ExportTable, Vec<String>) {
    let mut t = ExportTable::default();
    let mut local_names = vec![];
    let target_of_local = |local: &str| -> Option<ExportTarget> {
        if let Some((spec, name)) = x.imports_map.get(local) {
            return Some(match name {
                ImportName::Namespace => ExportTarget::Namespace(spec.clone()),
                n => ExportTarget::Reexport(spec.clone(), n.clone()),
            });
        }
        x.top_names
            .get(local)
            .map(|id| ExportTarget::Node(id.clone()))
    };
    for e in &mr.local_export_entries {
        let export = match &e.export_name {
            ExportExportName::Name(n) => n.name.as_str().to_string(),
            ExportExportName::Default(_) => "default".into(),
            ExportExportName::Null => continue,
        };
        let local = match &e.local_name {
            ExportLocalName::Name(n) => n.name.as_str().to_string(),
            _ => x.default_ident.clone().unwrap_or_else(|| "default".into()),
        };
        local_names.push(local.clone());
        if let Some(target) = target_of_local(&local) {
            t.names.insert(export, target);
        }
    }
    for e in &mr.indirect_export_entries {
        let Some(spec) = &e.module_request else {
            continue;
        };
        let spec = spec.name.as_str().to_string();
        let export = match &e.export_name {
            ExportExportName::Name(n) => n.name.as_str().to_string(),
            ExportExportName::Default(_) => "default".into(),
            ExportExportName::Null => continue,
        };
        let target = match &e.import_name {
            ExportImportName::Name(n) => {
                ExportTarget::Reexport(spec, ImportName::named(n.name.as_str()))
            }
            ExportImportName::All | ExportImportName::AllButDefault => {
                ExportTarget::Namespace(spec)
            }
            ExportImportName::Null => continue,
        };
        t.names.insert(export, target);
    }
    for e in &mr.star_export_entries {
        if let Some(spec) = &e.module_request {
            t.stars.push(spec.name.as_str().to_string());
        }
    }
    (t, local_names)
}
