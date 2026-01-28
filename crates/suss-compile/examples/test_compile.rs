use suss_compile::Compiler;

fn main() {
    let mut c = Compiler::new();
    
    // Test 1: Simple expression (should work)
    match c.compile_expr("(+ 1 2)") {
        Ok(w) => println!("Test 1 OK: {} bytes", w.len()),
        Err(e) => println!("Test 1 FAIL: {}", e),
    }
    
    // Test 2: defn + call
    let mut c2 = Compiler::new();
    match c2.compile_expr("(defn foo [x] (+ x 1)) (foo 5)") {
        Ok(w) => println!("Test 2 OK: {} bytes", w.len()),
        Err(e) => println!("Test 2 FAIL: {}", e),
    }
    
    // Test 3: pr-str wrapped defn
    let mut c3 = Compiler::new();
    match c3.compile_expr("(pr-str (defn foo [x] (+ x 1)) (foo 5))") {
        Ok(w) => println!("Test 3 OK: {} bytes", w.len()),
        Err(e) => println!("Test 3 FAIL: {}", e),
    }
}
