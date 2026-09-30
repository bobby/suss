//! Source namespace directives against an explicit declaration catalog.
//! This is not a file loader or a compiled macro-session implementation.
use super::{
    Diagnostic,
    resolve::{Environment, Phase},
};
use suss_reader::forms::{Form, Kind};

fn error(form: &Form, message: &str) -> Diagnostic {
    Diagnostic {
        span: form.span.clone(),
        message: message.into(),
    }
}
fn located<T>(result: Result<T, Diagnostic>, form: &Form) -> Result<T, Diagnostic> {
    result.map_err(|mut error| {
        error.span = form.span.clone();
        error
    })
}
fn symbol(form: &Form) -> Result<&str, Diagnostic> {
    match &form.kind {
        Kind::Symbol(value) if value.namespace.is_none() => Ok(&value.name),
        _ => Err(error(form, "Expected unqualified symbol")),
    }
}
fn keyword(form: &Form) -> Result<&str, Diagnostic> {
    match &form.kind {
        Kind::Keyword(value) if value.namespace.is_none() => Ok(&value.name),
        _ => Err(error(form, "Expected unqualified namespace option keyword")),
    }
}
fn sequence(form: &Form) -> Result<&[Form], Diagnostic> {
    match &form.kind {
        Kind::List(items) | Kind::Vector(items) => Ok(items),
        _ => Err(error(form, "Expected namespace symbol sequence")),
    }
}
fn renames(form: &Form) -> Result<Vec<(&Form, &Form)>, Diagnostic> {
    let Kind::Map(items) = &form.kind else {
        return Err(error(form, "Namespace renames must be a map"));
    };
    if items.len() % 2 != 0 {
        return Err(error(form, "Namespace rename has no target"));
    }
    let mut names = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for pair in items.chunks_exact(2) {
        if !names.insert(symbol(&pair[0])?) {
            return Err(error(&pair[0], "Duplicate namespace rename"));
        }
        symbol(&pair[1])?;
        out.push((&pair[0], &pair[1]));
    }
    Ok(out)
}
fn require(env: &mut Environment, phase: Phase, spec: &Form) -> Result<(), Diagnostic> {
    let items = match &spec.kind {
        Kind::Symbol(_) => std::slice::from_ref(spec),
        Kind::Vector(items) | Kind::List(items) if !items.is_empty() => items,
        _ => return Err(error(spec, "Require expects a namespace symbol or libspec")),
    };
    let namespace = symbol(&items[0])?;
    if !env.has_namespace(phase, namespace) {
        return Err(error(
            &items[0],
            "Required namespace has no supplied declaration; source loading is not integrated yet",
        ));
    }
    if items.len() % 2 == 0 {
        return Err(error(spec, "Require option has no value"));
    }
    let mut options = std::collections::BTreeSet::new();
    let mut alias = None;
    let mut referred = Vec::new();
    let mut renamed = Vec::new();
    for pair in items[1..].chunks_exact(2) {
        let key = keyword(&pair[0])?;
        if !options.insert(key) {
            return Err(error(&pair[0], "Duplicate require option"));
        }
        match key {
            "as" => {
                symbol(&pair[1])?;
                alias = Some(&pair[1]);
            }
            "refer" => {
                let mut names = std::collections::BTreeSet::new();
                for name in sequence(&pair[1])? {
                    if !names.insert(symbol(name)?) {
                        return Err(error(name, "Duplicate referred name"));
                    }
                    referred.push(name);
                }
            }
            "rename" => renamed = renames(&pair[1])?,
            _ => return Err(error(&pair[0], "Require option is not supported")),
        }
    }
    if let Some(alias) = alias {
        located(env.alias(phase, symbol(alias)?, namespace), alias)?;
    }
    for (original, _) in &renamed {
        if !referred
            .iter()
            .any(|name| symbol(name).ok() == symbol(original).ok())
        {
            return Err(error(original, "Renamed name must be referred"));
        }
    }
    for original in referred {
        let name = symbol(original)?;
        let target = renamed
            .iter()
            .find(|(old, _)| symbol(old).ok() == Some(name))
            .map_or(original, |(_, new)| *new);
        located(env.refer(phase, symbol(target)?, namespace, name), target)?;
    }
    Ok(())
}
fn core_options(
    env: &mut Environment,
    phase: Phase,
    items: &[Form],
    form: &Form,
) -> Result<(), Diagnostic> {
    if items.len() % 2 != 0 {
        return Err(error(form, "Core refer option has no value"));
    }
    let mut options = std::collections::BTreeSet::new();
    for pair in items.chunks_exact(2) {
        let key = keyword(&pair[0])?;
        if !options.insert(key) {
            return Err(error(&pair[0], "Duplicate core refer option"));
        }
        match key {
            "exclude" => {
                for name in sequence(&pair[1])? {
                    located(env.exclude_core(phase, symbol(name)?), name)?;
                }
            }
            "rename" => {
                for (old, new) in renames(&pair[1])? {
                    located(env.exclude_core(phase, symbol(old)?), old)?;
                    located(
                        env.refer(phase, symbol(new)?, "suss.core", symbol(old)?),
                        new,
                    )?;
                }
            }
            _ => return Err(error(&pair[0], "Core refer option is not supported")),
        }
    }
    Ok(())
}
/// Only a leading top-level ns directive is currently consumed. All mutations
/// occur in the caller's private compilation snapshot, never its live catalog.
pub(crate) fn namespace(
    forms: &mut [Form],
    env: &mut Environment,
    phase: Phase,
) -> Result<Option<Form>, Diagnostic> {
    let Some(form) = forms.first_mut() else {
        return Ok(None);
    };
    let Kind::List(items) = &form.kind else {
        return Ok(None);
    };
    if !matches!(items.first().map(|form| &form.kind), Some(Kind::Symbol(name)) if name.namespace.is_none() && name.name == "ns")
    {
        return Ok(None);
    }
    let Some(name) = items.get(1) else {
        return Err(error(form, "Namespace name is required"));
    };
    located(env.reset_namespace(phase, symbol(name)?), name)?;
    let mut rest = &items[2..];
    if rest
        .first()
        .is_some_and(|form| matches!(form.kind, Kind::String(_)))
    {
        rest = &rest[1..];
    }
    if rest
        .first()
        .is_some_and(|form| matches!(form.kind, Kind::Map(_)))
    {
        rest = &rest[1..];
    }
    let mut clauses = std::collections::BTreeSet::new();
    for clause in rest {
        let Kind::List(items) = &clause.kind else {
            return Err(error(clause, "Namespace clause must be a list"));
        };
        let Some(head) = items.first() else {
            return Err(error(clause, "Namespace clause is empty"));
        };
        let key = keyword(head)?;
        if !clauses.insert(key) {
            return Err(error(clause, "Duplicate namespace clause"));
        }
        match key {
            "require" => {
                for spec in &items[1..] {
                    require(env, phase, spec)?;
                }
            }
            "refer-clojure" => {
                core_options(env, phase, &items[1..], clause)?;
            }
            "require-macros" => {
                return Err(error(
                    clause,
                    "Source macro imports require an isolated compiled macro session, not yet integrated",
                ));
            }
            _ => return Err(error(head, "Namespace clause is not supported")),
        }
    }
    let directive = form.clone();
    form.kind = Kind::Nil;
    Ok(Some(directive))
}
