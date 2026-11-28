use suss_reader::{forms::{read_forms,resolve_conditionals},edn::{Value,source::form_to_value}};
fn main(){let forms=resolve_conditionals(read_forms(r##"#{#"a" #"a"}"##).unwrap()).unwrap();let value=form_to_value(&forms[0]).unwrap();let Value::Set(items)=value else{panic!("set")};assert_eq!(items.len(),2,"distinct Pattern objects must remain distinct tooling keys");}
