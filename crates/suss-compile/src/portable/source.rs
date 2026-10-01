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
fn requirement(spec: &Form) -> Result<Requirement<'_>, Diagnostic> {
    // Pinned ns analysis reads reload policy from libspec metadata. Retaining
    // the directive cannot implement its required initialization behavior.
    for metadata in &spec.metadata {
        let reload = |form: &Form| {
            matches!(&form.kind, Kind::Keyword(key)
                if key.namespace.is_none() && key.name == "reload")
        };
        if reload(metadata)
            || matches!(&metadata.kind, Kind::Map(entries)
                if entries.chunks_exact(2).any(|entry| reload(&entry[0])))
        {
            return Err(error(
                spec,
                "Require reload metadata needs source loading and initialization policy, not yet integrated",
            ));
        }
    }
    let items = match &spec.kind {
        Kind::Symbol(_) => std::slice::from_ref(spec),
        Kind::Vector(items) | Kind::List(items) if !items.is_empty() => items,
        _ => return Err(error(spec, "Require expects a namespace symbol or libspec")),
    };
    symbol(&items[0])?;
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
    for (original, _) in &renamed {
        if !referred
            .iter()
            .any(|name| symbol(name).ok() == symbol(original).ok())
        {
            return Err(error(original, "Renamed name must be referred"));
        }
    }
    Ok(Requirement {
        namespace: &items[0],
        alias,
        referred,
        renamed,
    })
}
struct Requirement<'a> {
    namespace: &'a Form,
    alias: Option<&'a Form>,
    referred: Vec<&'a Form>,
    renamed: Vec<(&'a Form, &'a Form)>,
}
fn apply_requirement(
    env: &mut Environment,
    phase: Phase,
    requirement: &Requirement<'_>,
) -> Result<(), Diagnostic> {
    let namespace = symbol(requirement.namespace)?;
    if !env.has_namespace(phase, namespace) {
        return Err(error(
            requirement.namespace,
            "Required namespace has no supplied declaration; use module graph preparation for source loading",
        ));
    }
    if let Some(alias) = requirement.alias {
        located(env.alias(phase, symbol(alias)?, namespace), alias)?;
    }
    for original in &requirement.referred {
        let name = symbol(original)?;
        let target = requirement
            .renamed
            .iter()
            .find(|(old, _)| symbol(old).ok() == Some(name))
            .map_or(*original, |(_, new)| *new);
        located(env.refer(phase, symbol(target)?, namespace, name), target)?;
    }
    Ok(())
}
enum CoreOption<'a> {
    Exclude(Vec<&'a Form>),
    Rename(Vec<(&'a Form, &'a Form)>),
}
fn core_options<'a>(items: &'a [Form], form: &Form) -> Result<Vec<CoreOption<'a>>, Diagnostic> {
    if items.len() % 2 != 0 {
        return Err(error(form, "Core refer option has no value"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut options = Vec::new();
    for pair in items.chunks_exact(2) {
        let key = keyword(&pair[0])?;
        if !seen.insert(key) {
            return Err(error(&pair[0], "Duplicate core refer option"));
        }
        options.push(match key {
            "exclude" => {
                let names = sequence(&pair[1])?;
                for name in names {
                    symbol(name)?;
                }
                CoreOption::Exclude(names.iter().collect())
            }
            "rename" => CoreOption::Rename(renames(&pair[1])?),
            _ => return Err(error(&pair[0], "Core refer option is not supported")),
        });
    }
    Ok(options)
}
enum Clause<'a> {
    Require(Vec<Requirement<'a>>),
    Macros(Vec<Requirement<'a>>),
    Core(Vec<CoreOption<'a>>),
}
struct Header<'a> {
    name: &'a Form,
    clauses: Vec<Clause<'a>>,
}
fn header_with_phases(form: &Form, macro_imports: bool) -> Result<Option<Header<'_>>, Diagnostic> {
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
    located(super::resolve::valid_namespace(symbol(name)?), name)?;
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
    let mut seen = std::collections::BTreeSet::new();
    let mut clauses = Vec::new();
    for clause in rest {
        let Kind::List(items) = &clause.kind else {
            return Err(error(clause, "Namespace clause must be a list"));
        };
        let Some(head) = items.first() else {
            return Err(error(clause, "Namespace clause is empty"));
        };
        let key = keyword(head)?;
        if !seen.insert(key) {
            return Err(error(clause, "Duplicate namespace clause"));
        }
        clauses.push(match key {
            "require" => Clause::Require(items[1..].iter().map(requirement).collect::<Result<_, _>>()?),
            "refer-clojure" => Clause::Core(core_options(&items[1..], clause)?),
            "require-macros" if macro_imports => Clause::Macros(items[1..].iter().map(requirement).collect::<Result<_, _>>()?),
            "require-macros" => return Err(error(clause, "Source macro imports require an isolated compiled macro session, not yet integrated")),
            _ => return Err(error(head, "Namespace clause is not supported")),
        });
    }
    Ok(Some(Header { name, clauses }))
}
/// A loader and ordinary source preparation share exactly the same header grammar.
pub(crate) fn input_header(
    forms: &[Form],
) -> Result<
    Option<(
        String,
        std::ops::Range<usize>,
        Vec<(String, std::ops::Range<usize>)>,
    )>,
    Diagnostic,
> {
    input_header_with_macros(forms, false)
}
pub(crate) fn input_header_with_macros(
    forms: &[Form],
    macro_imports: bool,
) -> Result<
    Option<(
        String,
        std::ops::Range<usize>,
        Vec<(String, std::ops::Range<usize>)>,
    )>,
    Diagnostic,
> {
    let Some(first) = forms.first() else {
        return Ok(None);
    };
    let Some(header) = header_with_phases(first, macro_imports)? else {
        return Ok(None);
    };
    let mut dependencies = Vec::new();
    for clause in &header.clauses {
        if let Clause::Require(requirements) = clause {
            for requirement in requirements {
                let name = symbol(requirement.namespace)?;
                located(super::resolve::valid_namespace(name), requirement.namespace)?;
                dependencies.push((name.to_owned(), requirement.namespace.span.clone()));
            }
        }
    }
    Ok(Some((
        symbol(header.name)?.to_owned(),
        header.name.span.clone(),
        dependencies,
    )))
}
pub(crate) fn dependencies(
    forms: &[Form],
) -> Result<
    (
        String,
        std::ops::Range<usize>,
        Vec<(String, std::ops::Range<usize>)>,
    ),
    Diagnostic,
> {
    input_header(forms)?.ok_or_else(|| Diagnostic {
        span: forms.first().map_or(0..0, |form| form.span.clone()),
        message: "Source module requires a leading ns declaration".into(),
    })
}
/// Consume a leading source declaration using a private compilation snapshot.
pub(crate) fn namespace(
    forms: &mut [Form],
    env: &mut Environment,
    phase: Phase,
    expander: &mut dyn super::ExpansionHost,
) -> Result<Option<Form>, Diagnostic> {
    let Some(form) = forms.first_mut() else {
        return Ok(None);
    };
    let Some(header) = header_with_phases(form, expander.supports_macro_imports())? else {
        return Ok(None);
    };
    located(
        env.reset_namespace(phase, symbol(header.name)?),
        header.name,
    )?;
    for clause in &header.clauses {
        match clause {
            Clause::Require(requirements) => {
                for requirement in requirements {
                    apply_requirement(env, phase, requirement)?;
                }
            }
            Clause::Macros(requirements) => {
                for requirement in requirements {
                    let namespace = symbol(requirement.namespace)?;
                    let exports = expander
                        .load_macro_namespace(namespace, requirement.namespace.span.clone())?;
                    env.declare_macro_exports(phase, namespace, &exports)?;
                    if let Some(alias) = requirement.alias {
                        located(env.macro_alias(phase, symbol(alias)?, namespace), alias)?;
                    }
                    for original in &requirement.referred {
                        let name = symbol(original)?;
                        let target = requirement
                            .renamed
                            .iter()
                            .find(|(old, _)| symbol(old).ok() == Some(name))
                            .map_or(*original, |(_, new)| *new);
                        located(
                            env.macro_refer(phase, symbol(target)?, namespace, name),
                            target,
                        )?;
                    }
                }
            }
            Clause::Core(options) => {
                for option in options {
                    match option {
                        CoreOption::Exclude(names) => {
                            for name in names {
                                located(env.exclude_core(phase, symbol(name)?), name)?;
                            }
                        }
                        CoreOption::Rename(names) => {
                            for (old, new) in names {
                                located(env.exclude_core(phase, symbol(old)?), old)?;
                                located(
                                    env.refer(phase, symbol(new)?, "suss.core", symbol(old)?),
                                    new,
                                )?;
                            }
                        }
                    }
                }
            }
        }
    }
    let directive = form.clone();
    form.kind = Kind::Nil;
    Ok(Some(directive))
}

/// Discover namespace edges without treating a Macro dependency as a Runtime
/// declaration or marking it initialized. Actual phase execution belongs to the
/// compiled host; ordinary preparation keeps rejecting unavailable macro imports.
pub(crate) fn phase_dependencies(
    forms: &[Form],
    phase: Phase,
) -> Result<
    (
        String,
        std::ops::Range<usize>,
        Vec<(Phase, String, std::ops::Range<usize>)>,
    ),
    Diagnostic,
> {
    let first = forms.first().ok_or_else(|| Diagnostic {
        span: 0..0,
        message: "Source module requires a leading ns declaration".into(),
    })?;
    let header = header_with_phases(first, true)?
        .ok_or_else(|| error(first, "Source module requires a leading ns declaration"))?;
    let mut dependencies = Vec::new();
    for clause in header.clauses {
        let (target_phase, requirements) = match clause {
            Clause::Require(requirements) => (phase, requirements),
            Clause::Macros(requirements) => (Phase::Macro, requirements),
            Clause::Core(_) => continue,
        };
        for requirement in requirements {
            let name = symbol(requirement.namespace)?;
            located(super::resolve::valid_namespace(name), requirement.namespace)?;
            dependencies.push((
                target_phase,
                name.to_owned(),
                requirement.namespace.span.clone(),
            ));
        }
    }
    Ok((
        symbol(header.name)?.into(),
        header.name.span.clone(),
        dependencies,
    ))
}
