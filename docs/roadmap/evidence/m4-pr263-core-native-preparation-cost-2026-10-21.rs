use suss_compile::portable::{self, Diagnostic, ExpansionContext, ExpansionHost, SourceOrigin};
use suss_reader::forms::{Form, read_forms_metered, resolve_conditionals_metered};
use std::{collections::{BTreeSet, HashMap}, ops::Range, path::PathBuf};
struct Count { remaining: u64, used: u64, calls: u64, spans: HashMap<(usize,usize),u64> }
impl ExpansionHost for Count {
 fn expansion_work_fuel(&self)->u64 {self.remaining}
 fn artifact_dependencies(&self)->Option<Vec<(String,String)>> {Some(vec![])}
 fn charge_expansion_work(&mut self, work:u64, span:Range<usize>)->Result<(),Diagnostic> {
  self.remaining=self.remaining.checked_sub(work).ok_or_else(||Diagnostic {span:span.clone(),message:"diagnostic native allowance exhausted".into()})?;
  self.used=self.used.checked_add(work).unwrap();self.calls+=1;*self.spans.entry((span.start,span.end)).or_default()+=work;Ok(())
 }
 fn expand(&mut self,_:&Form,_:ExpansionContext<'_>)->Result<Option<Form>,Diagnostic>{Ok(None)}
}
fn main(){
 let args:Vec<_>=std::env::args().collect();let source=std::fs::read_to_string(&args[1]).unwrap();let limit:u64=args[2].parse().unwrap();
 let mut count=Count{remaining:limit,used:0,calls:0,spans:HashMap::new()};
 let parsed=(||{
  let mut reader=|work|count.charge_expansion_work(work,0..0).map_err(|e|suss_reader::ParseError{message:e.message,span:e.span,expected:vec![]});
  let forms=read_forms_metered(&source,&mut reader)?;resolve_conditionals_metered(forms,&mut reader)
 })();
 let parsed=match parsed {Ok(v)=>v,Err(e)=>{eprintln!("reader error {e:?}");std::process::exit(2)}};
 count.charge_expansion_work(source.len() as u64,0..source.len()).unwrap();let prefix=count.used;
 let result=portable::modules::prepare_input_forms_with_origin(parsed,0..source.len(),&[PathBuf::from("src")],&portable::resolve::Environment::default(),portable::resolve::Phase::Runtime,&BTreeSet::new(),&mut count,Some(&SourceOrigin::new(source.as_str(),None)));
 let status=match result {Ok(input)=>format!("prepared modules={} fragment-ok",input.modules.len()),Err(e)=>format!("preparation error {e:?}")};
 let mut spans:Vec<_>=count.spans.into_iter().collect();spans.sort_by_key(|(_,work)|std::cmp::Reverse(*work));spans.truncate(10);
 println!("limit={limit} prefix_work={prefix} used={} remaining={} calls={} status={status}",count.used,count.remaining,count.calls);for(span,work)in spans{println!("span={}..{} work={work}",span.0,span.1);}
}
