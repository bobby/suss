// Copyright (c) Rich Hickey. All rights reserved.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
// which can be found in the file epl-v10.html at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0
//
// Parameter staging policy adapted from cljs/analyzer.cljc2208–2352 at
// c4295f303100bbf5afac449242d30bca1126f1a1. Upstream SHA-256:
// 297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47.
// Native identities and representation are original; no body is expanded here.
use super::*;

#[derive(Debug, Clone)]
pub struct SourceParameterMethod {
    pub parameters: Vec<LocalBinding>,
    pub variadic: bool,
}

/// Actual first-pass declarations, before self callable facts are installed.
#[derive(Debug, Clone)]
pub struct SourceFunctionParameters {
    pub methods: Vec<SourceParameterMethod>,
}
impl SourceFunctionParameters {
    pub fn max_fixed_arity(&self) -> usize {
        self.methods
            .iter()
            .map(|method| method.parameters.len() - usize::from(method.variadic))
            .max()
            .unwrap_or(0)
    }
}

impl Analyzer<'_> {
    pub(super) fn normalize_function_method(
        form: &Form,
        args: &[Form],
    ) -> Result<(Vec<Form>, bool, Option<usize>), Diagnostic> {
        let mut args = args.to_vec();
        let Some(Form {
            kind: Kind::Vector(names),
            ..
        }) = args.first_mut()
        else {
            return Err(fail(
                form.span.clone(),
                "Function requires a parameter vector",
            ));
        };
        let markers: Vec<_> = names.iter().enumerate().filter_map(|(index, name)|
            matches!(&name.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && symbol.name == "&").then_some(index)).collect();
        if let Some(&index) = markers.first() {
            if markers.len() != 1 || index + 2 != names.len() {
                return Err(fail(
                    form.span.clone(),
                    "Variadic signature requires exactly one trailing rest parameter",
                ));
            }
            names.remove(index);
        }
        Ok((args, !markers.is_empty(), markers.first().copied()))
    }

    /// Shared with body analysis: validate and allocate genuine declarations.
    pub(super) fn allocate_function_parameters(
        &mut self,
        names: &[Form],
        rest_parameter: Option<usize>,
        expression_context: bool,
    ) -> Result<(Vec<Parameter>, Vec<LocalBinding>), Diagnostic> {
        let mut parameters = Vec::new();
        let mut records = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let Kind::Symbol(symbol) = &name.kind else {
                return Err(fail(
                    name.span.clone(),
                    "Parameter destructuring is not lowered yet",
                ));
            };
            if symbol.namespace.is_some() || symbol.name == "&" {
                return Err(fail(
                    name.span.clone(),
                    "Parameters must be unqualified; variadic rest sequences are not lowered yet",
                ));
            }
            let id = BindingId(self.next);
            self.next += 1;
            self.insert_local(
                name,
                id,
                Type::Value,
                LocalKind::Argument {
                    index,
                    rest: rest_parameter == Some(index),
                },
                None,
            );
            let binding = self
                .locals
                .get_mut(&symbol.name)
                .expect("allocated parameter");
            if expression_context {
                binding.declaration_context = super::super::AnalysisContext::Expression;
            }
            records.push(binding.clone());
            parameters.push(Parameter {
                id,
                name: symbol.name.clone(),
                metadata: name.metadata.clone(),
                span: name.span.clone(),
            });
        }
        Ok((parameters, records))
    }

    pub(super) fn stage_function_parameters(
        &mut self,
        form: &Form,
        signatures: &[Form],
    ) -> Result<SourceFunctionParameters, Diagnostic> {
        let single = signatures
            .first()
            .is_some_and(|arg| matches!(arg.kind, Kind::Vector(_)));
        let methods = if single {
            vec![(form, signatures)]
        } else {
            signatures
                .iter()
                .map(|signature| {
                    let Kind::List(items) = &signature.kind else {
                        return Err(fail(
                            signature.span.clone(),
                            "Function signature requires a parameter vector and body",
                        ));
                    };
                    Ok((signature, items.as_slice()))
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let outer = self.locals.clone();
        let result = (|| {
            let mut records = Vec::new();
            for (method, args) in &methods {
                self.locals = outer.clone();
                let (args, variadic, rest) = Self::normalize_function_method(method, args)?;
                let Kind::Vector(names) = &args[0].kind else {
                    unreachable!()
                };
                let (_, parameters) =
                    self.allocate_function_parameters(names, rest, methods.len() > 1)?;
                records.push(SourceParameterMethod {
                    parameters,
                    variadic,
                });
            }
            Ok(SourceFunctionParameters { methods: records })
        })();
        self.locals = outer;
        result
    }
}
