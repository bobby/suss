//! Authoring regressions: uncompiled/unexecuted until the exclusive Cargo lane
//! is released. These assert typed compiler facts, not full reify support.
use std::{collections::HashMap, sync::Arc};
use suss_compile::portable::{
    compiler_facts::{elide_reader_metadata, CompilerNamespaceCatalog},
    compiler_names::{munge_name, munge_name_with_reserved},
    hir::{BindingId, LocalBinding, LocalKind, SourceBinding, SourceNamespace, SourceRole, Type},
    resolve::{Environment, Phase},
    AnalysisContext, ExpansionContext,
};
use suss_reader::forms::{read_forms, Kind};

#[test]
fn compiler_facts_resolve_actual_phase_aliases_and_declaration_identity() {
    let mut env = Environment::new("record.user").unwrap();
    let runtime = env
        .declare_cell(Phase::Runtime, "record.user", "value")
        .unwrap();
    let macro_value = env
        .declare_cell(Phase::Macro, "record.user", "value")
        .unwrap();
    env.alias(Phase::Runtime, "own", "record.user").unwrap();
    let snapshot = Arc::new(SourceNamespace::capture(&env, Phase::Runtime));
    let locals = HashMap::new();
    let fields = HashMap::new();
    let context = ExpansionContext {
        environment: &env,
        namespace_snapshot: &snapshot,
        origin: None,
        phase: Phase::Runtime,
        context: AnalysisContext::Expression,
        locals: &locals,
        fields: &fields,
        function_scopes: &[],
    };
    let Kind::Symbol(symbol) = read_forms("own/value").unwrap().remove(0).kind else {
        unreachable!()
    };
    let SourceBinding::Global {
        global,
        declaration,
    } = context.resolve_binding_fact(&symbol, 4..13, false).unwrap()
    else {
        panic!("global fact")
    };
    assert_eq!(global, runtime);
    assert_ne!(global, macro_value);
    assert!(
        declaration.is_none(),
        "cell declaration is not fabricated source syntax"
    );
    let Kind::Symbol(missing) = read_forms("own/missing").unwrap().remove(0).kind else {
        unreachable!()
    };
    let error = context
        .resolve_binding_fact(&missing, 7..18, true)
        .unwrap_err();
    assert_eq!(error.span, 7..18);
    assert!(error.message.contains("Unresolved"));
    assert!(context.lexical_bindings().is_empty());
}

#[test]
fn reader_meta_elision_retains_user_values_and_qualified_lookalikes() {
    let form = read_forms("^{:file \"x\" :line 3 :column 2 :end-line 4 :end-column 7 :source \"raw\" :user/line 8 :marker false :tag T} x").unwrap().remove(0);
    let pairs = elide_reader_metadata(&form).unwrap();
    let mut retained = form.clone();
    retained.metadata = vec![suss_reader::forms::Form {
        span: 0..0,
        metadata: vec![],
        kind: Kind::Map(pairs),
    }];
    let expected = read_forms("^{:user/line 8 :marker false :tag T} x")
        .unwrap()
        .remove(0);
    let actual = suss_compile::portable::hir::reader_metadata_pairs(&retained).unwrap();
    let expected = suss_compile::portable::hir::reader_metadata_pairs(&expected).unwrap();
    assert_eq!(
        actual.iter().map(|f| &f.kind).collect::<Vec<_>>(),
        expected.iter().map(|f| &f.kind).collect::<Vec<_>>()
    );
}

#[test]
fn unused_lexical_bindings_keep_declaration_identity_and_shadow_globals() {
    let mut env = Environment::new("record.user").unwrap();
    let global = env
        .declare_cell(Phase::Runtime, "record.user", "unused")
        .unwrap();
    let declaration = read_forms("unused").unwrap().remove(0);
    let identity = Arc::new(());
    let local = LocalBinding {
        identity: identity.clone(),
        id: BindingId(17),
        ty: Type::Value,
        declaration: declaration.clone(),
        origin: None,
        kind: LocalKind::Let,
        declaration_context: AnalysisContext::Expression,
        source_role: SourceRole::Plain,
        initializer: None,
        shadow: None,
        shadow_field: None,
    };
    let locals = HashMap::from([("unused".to_string(), local)]);
    let fields = HashMap::new();
    let snapshot = Arc::new(SourceNamespace::capture(&env, Phase::Runtime));
    let context = ExpansionContext {
        environment: &env,
        namespace_snapshot: &snapshot,
        origin: None,
        phase: Phase::Runtime,
        context: AnalysisContext::Expression,
        locals: &locals,
        fields: &fields,
        function_scopes: &[],
    };
    let Kind::Symbol(symbol) = &declaration.kind else {
        unreachable!()
    };
    let SourceBinding::Local(binding) = context.resolve_binding_fact(symbol, 0..6, true).unwrap()
    else {
        panic!("local fact")
    };
    assert!(Arc::ptr_eq(&binding.identity, &identity));
    assert_eq!(binding.id, BindingId(17));
    assert_eq!(
        context.lexical_bindings().len(),
        1,
        "unused capture retained"
    );
    let SourceBinding::Global {
        global: resolved, ..
    } = context.resolve_binding_fact(symbol, 0..6, false).unwrap()
    else {
        panic!("global fact")
    };
    assert_eq!(resolved, global);
}

#[test]
fn compiler_namespace_catalog_preserves_missing_scopes_phases_and_revisions() {
    let mut env = Environment::new("record.user").unwrap();
    env.declare_namespace(Phase::Runtime, "declared.only")
        .unwrap();
    env.enter_namespace(Phase::Runtime, "other.live").unwrap();
    let value = env
        .declare_cell(Phase::Runtime, "other.live", "value")
        .unwrap();
    env.enter_namespace(Phase::Runtime, "record.user").unwrap();
    env.alias(Phase::Runtime, "other", "other.live").unwrap();
    env.declare_namespace(Phase::Macro, "macro.only").unwrap();
    let catalog = CompilerNamespaceCatalog::capture(&env, Phase::Runtime);
    assert_eq!(env.current_namespace(Phase::Runtime), "record.user");
    assert_eq!(catalog.current, "record.user");
    assert!(catalog.namespaces["declared.only"].is_none());
    assert!(!catalog.namespaces.contains_key("macro.only"));
    let live = catalog.namespaces["other.live"].as_ref().unwrap();
    assert_eq!(live.identities, vec![value]);
    assert!(
        live.declarations.is_empty(),
        "cell is not invented source syntax"
    );
    assert_eq!(
        catalog.namespaces["record.user"].as_ref().unwrap().aliases["other"],
        "other.live"
    );
    assert!(catalog.starts_with_namespace_segment("other"));
    assert!(!catalog.starts_with_namespace_segment("oth"));
    assert!(!catalog.starts_with_namespace_segment("other.live"));
    env.enter_namespace(Phase::Runtime, "new.live").unwrap();
    assert!(
        !catalog.namespaces.contains_key("new.live"),
        "immutable snapshot"
    );
    assert!(CompilerNamespaceCatalog::capture(&env, Phase::Runtime)
        .namespaces
        .contains_key("new.live"));
    assert!(SourceNamespace::capture_namespace(&env, Phase::Runtime, "missing").is_none());
}

#[test]
fn compiler_name_munging_preserves_utf16_and_compiler_division_rules() {
    for (source, expected) in [
        ("record-name.class", "record_name.class$"),
        ("a..b", "a_DOT__DOT_b"),
        ("a/b", "a.b"),
        ("a/", "a_SLASH_"),
        ("a/\n", "a_SLASH_\n"),
        ("constructor", "constructor$"),
    ] {
        assert_eq!(
            munge_name(&source.encode_utf16().collect::<Vec<_>>()),
            expected.encode_utf16().collect::<Vec<_>>()
        );
    }
    assert_eq!(munge_name(&[0xd800, 45, 0xdc00]), vec![0xd800, 95, 0xdc00]);
    let mut visited = Vec::new();
    let custom = munge_name_with_reserved(&"a.class".encode_utf16().collect::<Vec<_>>(), |part| {
        visited.push(part.to_vec());
        part == [97]
    });
    assert_eq!(custom, "a$.class".encode_utf16().collect::<Vec<_>>());
    assert_eq!(
        visited,
        vec![vec![97], "class".encode_utf16().collect::<Vec<_>>()]
    );
}
