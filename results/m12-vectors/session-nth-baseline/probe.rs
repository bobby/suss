use suss_compile::portable_session::Session;
fn main() {
 for (phase, mut session) in [("Runtime", Session::new_repl().unwrap()), ("Macro", Session::new_macro().unwrap())] {
 let value=session.eval("(pr-str (with-redefs [nth (fn [& xs] 7)] (let [[a b] [1 2]] (+ a b))))").unwrap();
 let units=session.inspect(&value, |mut store,value| { let array=value.unwrap_anyref().unwrap().as_array(&store)?.unwrap(); Ok(array.elems(&mut store)?.map(|v|v.unwrap_i32() as u16).collect::<Vec<_>>()) }).unwrap();
 println!("{} {}",phase,String::from_utf16(&units).unwrap());
 }
}