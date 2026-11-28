#[path = "../../crates/suss-compile/src/portable/command/memory.rs"]
mod current;
#[path = "canonical-allocator-original.rs"]
mod original;
use wasm_encoder::{Component, RawSection};
use wasmparser::{Parser, Payload, Validator, WasmFeatures};
fn main() {
 let args=std::env::args().collect::<Vec<_>>();
 let input=std::fs::read(&args[1]).unwrap();
 let old=original::module();let new=current::module();
 let mut output=Component::new();let mut depth=0;let mut replaced=0;
 for payload in Parser::new(0).parse_all(&input) {
  let payload=payload.unwrap();
  match &payload {
   Payload::Version{..}=>{depth+=1;continue},
   Payload::End(_)=>{depth-=1;continue},
   _=>{},
  }
  if depth!=1 {continue}
  if let Some((id,range))=payload.as_section() {
   let bytes=&input[usize::try_from(range.start).unwrap()..usize::try_from(range.end).unwrap()];
   let data=if matches!(&payload,Payload::ModuleSection{..}) && bytes==old.as_slice() {
    replaced+=1;new.as_slice()
   }else{bytes};
   output.section(&RawSection{id,data});
  }
 }
 assert_eq!(replaced,1,"exactly one original owned allocator must match");
 let output=output.finish();
 Validator::new_with_features(WasmFeatures::all()).validate_all(&output).unwrap();
 // Prove every byte outside the single replaced core-module section survives.
 fn leb(mut value:usize)->Vec<u8>{let mut out=vec![];loop{let mut byte=(value&127)as u8;value>>=7;if value!=0{byte|=128}out.push(byte);if value==0{return out}}}
 let mut old_section=vec![1];old_section.extend(leb(old.len()));old_section.extend(&old);
 let mut new_section=vec![1];new_section.extend(leb(new.len()));new_section.extend(&new);
 let positions=input.windows(old_section.len()).enumerate().filter_map(|(i,v)|(v==old_section).then_some(i)).collect::<Vec<_>>();
 assert_eq!(positions.len(),1);
 let at=positions[0];let mut expected=input[..at].to_vec();expected.extend(&new_section);expected.extend(&input[at+old_section.len()..]);
 assert_eq!(output,expected,"business/core/canonical adapter bytes must otherwise be identical");
 std::fs::write(&args[2],&output).unwrap();
 std::fs::write(&args[3],&new).unwrap();
 println!("one exact owned allocator replaced: {} -> {} bytes; component {} -> {}; all other bytes conserved; validation passed",old.len(),new.len(),input.len(),output.len());
}
