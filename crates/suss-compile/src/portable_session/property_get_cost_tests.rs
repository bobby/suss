//! Test-only instruction-fuel observations; not a production fast path or wall-time benchmark.
use super::*;
use wasmtime::AsContext;
use serde_json::json;

fn scalar(s: &mut Session, x: portable::hir::Literal) -> SessionValue {
    s.data_scalar(&x).unwrap()
}

// All rooting, export resolution and result buffers precede the interval.
// Owning/decoding and pending-exception inspection follow it, even on errors.
fn measured(s: &mut Session, export: &str, owner: &SessionValue,
            key: Option<&SessionValue>, index: i32)
    -> (u64, Result<SessionValue, wasmtime::Error>) {
    let mut scope = RootScope::new(&mut s.store);
    let f = s.runtime.get_func(&mut scope, export).unwrap();
    let a = Val::AnyRef(Some(owner.value.to_rooted(&mut scope)));
    let b = match key {
        Some(k) => Val::AnyRef(Some(k.value.to_rooted(&mut scope))),
        None => Val::I32(index),
    };
    let args = [a, b];
    let mut out = [Val::null_any_ref()];
    let before = scope.as_context().get_fuel().unwrap();
    let result = f.call(&mut scope, &args, &mut out);
    let after = scope.as_context().get_fuel().unwrap();
    println!("PROPERTY_GET_CALL export={export} indexed={} fuel={} ok={}", key.is_none(), before-after, result.is_ok());
    let value = result.map(|()| own(&mut scope, out[0].clone(), s.identity, &s.handles).unwrap());
    (before.checked_sub(after).unwrap(), value)
}

fn raw_call(s: &mut Session, export: &str, values: &[&SessionValue]) -> SessionValue {
    let mut scope = RootScope::new(&mut s.store);
    let f = s.runtime.get_func(&mut scope, export).unwrap();
    let args: Vec<_> = values.iter().map(|v| Val::AnyRef(Some(v.value.to_rooted(&mut scope)))).collect();
    let mut out = [Val::null_any_ref()];
    f.call(&mut scope, &args, &mut out).unwrap();
    own(&mut scope, out[0].clone(), s.identity, &s.handles).unwrap()
}

fn view(s: &mut Session, v: &SessionValue) -> serde_json::Value {
    let mut scope = RootScope::new(&mut s.store);
    let r = v.value.to_rooted(&mut scope);
    if let Some(i) = r.as_i31(&scope).unwrap() { return json!({"i31":i.get_u32()}); }
    if let Some(a) = r.as_array(&scope).unwrap() {
        let units: Vec<_> = (0..a.len(&scope).unwrap()).map(|i| a.get(&mut scope,i).unwrap().unwrap_i32() as u16).collect();
        return json!({"utf16":units});
    }
    let object = r.as_struct(&scope).unwrap().unwrap();
    let first = object.field(&mut scope,0).unwrap();
    if let Val::F64(bits) = first { return json!({"number_bits":format!("{bits:016x}")}); }
    panic!("unexpected scalar result");
}

#[test]
fn exported_property_get_instruction_fuel() {
    use portable::hir::Literal;
    let mut rows = Vec::new();
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = match phase { Phase::Runtime => Session::new_repl(), Phase::Macro => Session::new_macro() }.unwrap();
        let strings: Vec<_> = ["", "abc", "😀é"].iter().map(|x|scalar(&mut s,Literal::String(x.encode_utf16().collect()))).collect();
        let keys: Vec<_> = ["length","charCodeAt","charAt","ns","name","_hash","not-present"].iter().map(|x|scalar(&mut s,Literal::String(x.encode_utf16().collect()))).collect();
        let keyword = s.eval("(keyword \"bench\" \"value\")").unwrap();
        let nil_ns_keyword = s.eval("(keyword \"value\")").unwrap();
        let literal_keyword = s.eval(":bench/value").unwrap();
        let nil = scalar(&mut s,Literal::Nil);
        let zero = scalar(&mut s,Literal::Number(0.0));
        // Runtime adapters require private ARGS, not data_array's source-array wrapper.
        let method_args = {
            let mut scope=RootScope::new(&mut s.store);
            let f=s.runtime.get_func(&mut scope,"args-new").unwrap();
            let mut out=[Val::null_any_ref()];
            f.call(&mut scope,&[Val::I32(1)],&mut out).unwrap();
            let array=out[0].unwrap_anyref().unwrap().as_array(&scope).unwrap().unwrap();
            let value=Val::AnyRef(Some(zero.value.to_rooted(&mut scope)));
            array.set(&mut scope,0,value).unwrap();
            own(&mut scope,out[0].clone(),s.identity,&s.handles).unwrap()
        };
        let _request = s.operation_budget_scope().unwrap();
        for (si,owner) in strings.iter().enumerate() {
            for (ki,key) in keys[..3].iter().enumerate() {
                for export in ["named-property-get","fixed-named-property-get"] {
                    for repeat in 0..3 {
                        let (fuel,result) = measured(&mut s,export,owner,Some(key),0);
                        let value = result.unwrap();
                        let decoded = if ki==0 { view(&mut s,&value) } else {
                            let answer=raw_call(&mut s,"object-method-invoke",&[&value,owner,&method_args]);
                            view(&mut s,&answer)
                        };
                        let expected = match ki {
                            0=>json!({"number_bits":format!("{:016x}",[0.0_f64,3.0,3.0][si].to_bits())}),
                            1=>json!({"number_bits":format!("{:016x}",[f64::NAN,97.0,55357.0][si].to_bits())}),
                            _=>json!({"utf16":match si {0=>vec![],1=>vec![97],_=>vec![55357]}}),
                        };
                        if ki==1 && si==0 {
                            let bits=u64::from_str_radix(decoded["number_bits"].as_str().unwrap(),16).unwrap();
                            assert!(f64::from_bits(bits).is_nan());
                        } else { assert_eq!(decoded,expected); }
                        rows.push(json!({"phase":format!("{phase:?}"),"owner":(["empty","ascii","utf16"][si]),"property":(["length","charCodeAt","charAt"][ki]),"export":export,"repeat":repeat,"fuel":fuel,"validated":decoded}));
                    }
                }
            }
        }
        for (owner,label) in [(&keyword,"keyword-dynamic-ns"),(&nil_ns_keyword,"keyword-dynamic-nil-ns"),(&literal_keyword,"keyword-literal")] {
            for (ki,index) in [(3,0),(4,1),(5,3)] {
                // Indexed getter is a measured comparison, not permission to omit live schema/name checks.
                let (_,expected) = measured(&mut s,"object-field-get",owner,None,index);
                let expected = view(&mut s,&expected.unwrap());
                for export in ["named-property-get","fixed-named-property-get","object-field-get"] {
                    for repeat in 0..3 {
                        let (fuel,result) = measured(&mut s,export,owner,if export=="object-field-get"{None}else{Some(&keys[ki])},index);
                        let decoded=view(&mut s,&result.unwrap());
                        assert_eq!(decoded,expected);
                        if ki==3 {assert_eq!(decoded,if label=="keyword-dynamic-nil-ns" {json!({"i31":0})} else {json!({"utf16":[98,101,110,99,104]})});}
                        if ki==4 {assert_eq!(decoded,json!({"utf16":[118,97,108,117,101]}));}
                        if ki==5 && label!="keyword-literal" {assert_eq!(decoded,json!({"i31":0}));}
                        rows.push(json!({"phase":format!("{phase:?}"),"owner":label,"property":(["ns","name","_hash"][ki-3]),"export":export,"repeat":repeat,"fuel":fuel,"validated":decoded}));
                    }
                }
            }
        }
        for (label,owner,key) in [("string-unknown",&strings[1],&keys[6]),("keyword-unknown",&keyword,&keys[6]),("nil-owner",&nil,&keys[0]),("nonstring-key",&strings[1],&zero)] {
            for export in ["named-property-get","fixed-named-property-get"] {
                let (fuel,result)=measured(&mut s,export,owner,Some(key),0);
                let outcome=match result {
                    Ok(v)=>{let decoded=view(&mut s,&v);assert!(label.ends_with("unknown"));assert_eq!(decoded,json!({"i31":6}));decoded},
                    Err(e)=>{assert!(!label.ends_with("unknown"),"{label}: {e:#}");assert!(e.is::<wasmtime::ThrownException>(),"{e:#}");let pending=s.store.take_pending_exception().expect("original pending exception");assert!(wasmtime::Tag::eq(&pending.tag(&mut s.store).unwrap(),&s.runtime.get_tag(&mut s.store,"language-exception").unwrap(),&s.store));json!({"error":"original-language-exception","raw":format!("{e:#}")})},
                };
                rows.push(json!({"phase":format!("{phase:?}"),"case":label,"export":export,"fuel":fuel,"validated":outcome}));
            }
        }
    }
    let output=json!({"measurement":"get_fuel immediately around exact exported Func.call only","rows":rows,"limits":"Getter cost only: method invocation/decoding are outside intervals; repeated calls are ordered, not isolated cold starts. Indexed getters retain different guards and cannot replace named access by inference. No guest hash invoked."});
    println!("PROPERTY_GET_COST {output}");
    if let Some(path)=std::env::var_os("SUSS_PROPERTY_GET_COST_OUTPUT") {std::fs::write(path,serde_json::to_vec_pretty(&output).unwrap()).unwrap();}
}

fn private_args(s: &mut Session, v: &SessionValue) -> SessionValue {
    let mut scope=RootScope::new(&mut s.store);
    let f=s.runtime.get_func(&mut scope,"args-new").unwrap();
    let mut out=[Val::null_any_ref()];
    f.call(&mut scope,&[Val::I32(1)],&mut out).unwrap();
    let a=out[0].unwrap_anyref().unwrap().as_array(&scope).unwrap().unwrap();
    let v=Val::AnyRef(Some(v.value.to_rooted(&mut scope)));
    a.set(&mut scope,0,v).unwrap();
    own(&mut scope,out[0].clone(),s.identity,&s.handles).unwrap()
}

fn measured_call(s: &mut Session, export: &str, values: &[&SessionValue]) -> (u64,SessionValue) {
    let mut scope=RootScope::new(&mut s.store);
    let f=s.runtime.get_func(&mut scope,export).unwrap();
    let args:Vec<_>=values.iter().map(|v|Val::AnyRef(Some(v.value.to_rooted(&mut scope)))).collect();
    let mut out=[Val::null_any_ref()];
    let before=scope.as_context().get_fuel().unwrap();
    let result=f.call(&mut scope,&args,&mut out);
    let after=scope.as_context().get_fuel().unwrap();
    result.unwrap();
    let v=own(&mut scope,out[0].clone(),s.identity,&s.handles).unwrap();
    (before-after,v)
}

#[test]
fn exported_char_code_at_instruction_fuel() {
    use portable::hir::Literal;
    let mut rows=Vec::new();
    for phase in [Phase::Runtime,Phase::Macro] {
        let mut s=match phase {Phase::Runtime=>Session::new_repl(),Phase::Macro=>Session::new_macro()}.unwrap();
        let key=scalar(&mut s,Literal::String("charCodeAt".encode_utf16().collect()));
        let values=[f64::NEG_INFINITY,-1.0,-0.0,0.0,0.9,1.0,1.9,2.0,3.0,4294967296.0,f64::INFINITY,f64::NAN];
        let strings:Vec<_>=["","abc","😀é"].iter().map(|v|scalar(&mut s,Literal::String(v.encode_utf16().collect()))).collect();
        let numbers:Vec<_>=values.iter().map(|v|scalar(&mut s,Literal::Number(*v))).collect();
        let buffers:Vec<_>=numbers.iter().map(|v|private_args(&mut s,v)).collect();
        let _request=s.operation_budget_scope().unwrap();
        for (si,owner) in strings.iter().enumerate() {
            let units:Vec<_>=["","abc","😀é"][si].encode_utf16().collect();
            let method=raw_call(&mut s,"named-property-get",&[owner,&key]);
            for (ni,index) in numbers.iter().enumerate() {
                for repeat in 0..3 {
                    let (lookup_fuel,selected)=measured(&mut s,"named-property-get",owner,Some(&key),0);
                    let selected=selected.unwrap();
                    let (object_fuel,result)=measured_call(&mut s,"object-method-invoke",&[&selected,owner,&buffers[ni]]);
                    let (scalar_fuel,direct)=measured_call(&mut s,"string-char-code-at",&[owner,index]);
                    // Validate independently from Rust UTF16 units and JS ToIntegerOrInfinity.
                    let n=if values[ni].is_nan(){0.0}else{values[ni].trunc()};
                    let expected=if n>=0.0 && n<(units.len() as f64){f64::from(units[n as usize])}else{f64::NAN};
                    let a=view(&mut s,&result);let b=view(&mut s,&direct);
                    for observed in [&a,&b] {
                        let bits=u64::from_str_radix(observed["number_bits"].as_str().unwrap(),16).unwrap();
                        if expected.is_nan(){assert!(f64::from_bits(bits).is_nan());}else{assert_eq!(bits,expected.to_bits());}
                    }
                    // Method-only cost uses the exact already selected method and same private ARGS.
                    let (selected_object_fuel,selected_result)=measured_call(&mut s,"object-method-invoke",&[&method,owner,&buffers[ni]]);
                    assert_eq!(view(&mut s,&selected_result),a);
                    assert_eq!(selected_object_fuel,object_fuel);
                    rows.push(json!({"phase":format!("{phase:?}"),"owner":(["empty","ascii","utf16"][si]),"index_bits":format!("{:016x}",values[ni].to_bits()),"repeat":repeat,"lookup_fuel":lookup_fuel,"object_invoke_fuel":object_fuel,"scalar_fuel":scalar_fuel,"lookup_plus_invoke_fuel":lookup_fuel+object_fuel,"validated":a}));
                }
            }
        }
    }
    let result=json!({"rows":rows,"limits":"Exact existing exported Func.call intervals. Input evaluation, rooting, export lookup, private ARGS allocation, validation and compiler LocalGet/ArrayNewFixed overhead excluded. lookup_plus_invoke is a sum of disjoint actual call intervals, not compiled full-expression fuel. Existing scalar coercion preserved. Primitive STRING and boxed NUMBER only; no fallback/custom receiver safety proof, no codegen/tariff/quota change."});
    println!("CHAR_CODE_AT_COST {result}");
    if let Some(path)=std::env::var_os("SUSS_PROPERTY_GET_COST_OUTPUT"){std::fs::write(path,serde_json::to_vec_pretty(&result).unwrap()).unwrap();}
}
