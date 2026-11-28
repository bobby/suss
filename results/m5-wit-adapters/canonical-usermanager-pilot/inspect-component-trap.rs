use wasmparser::{Parser, Payload, Encoding};
fn main() {
 let bytes=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
 let points=[0x459e9f,0x459fbe,0x45816f,0x3d26b0,0x3f7887,0x414a38,0x2e695,0x4116bb,0x4442b8];
 let mut stack=vec![];let mut next=0;let mut ordinals=std::collections::BTreeMap::new();
 let mut exports=std::collections::BTreeMap::<usize,Vec<String>>::new();let mut hit=std::collections::BTreeSet::new();
 for payload in Parser::new(0).parse_all(&bytes) {
  match payload.unwrap() {
   Payload::Version{encoding,range,..}=> {let id=if encoding==Encoding::Module {let id=next;next+=1;println!("module {id} starts {}",range.start);Some(id)}else{None};stack.push(id);}
   Payload::End(_)=>{stack.pop();}
   Payload::ExportSection(reader)=>if let Some(Some(id))=stack.last(){for e in reader{let e=e.unwrap();exports.entry(*id).or_default().push(format!("{} {:?} {}",e.name,e.kind,e.index));}},
   Payload::CodeSectionEntry(body)=>if let Some(Some(id))=stack.last(){
    let ordinal=ordinals.entry(*id).or_insert(0usize);
    let inside=points.iter().any(|p|body.range().contains(p));
    if inside {hit.insert(*id);println!("HIT module {id} body {ordinal} range {:?}",body.range());
     let mut ops=body.get_operators_reader().unwrap();
     while !ops.eof() {let offset=ops.original_position();let op=ops.read().unwrap();if points.iter().any(|p|offset.abs_diff(*p)<36){println!("  {offset:#x} {op:?}");}}
    } *ordinal+=1;
   },
   _=>{}
  }
 }
 for id in hit {println!("HIT module {id} exports {:?}",exports.get(&id));}
}
