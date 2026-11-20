//! Expected effects were observed on JVM Clojure 1.12.6 (#482).
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session.eval(&format!("(pr-str {source})"))
        .unwrap_or_else(|error| panic!("{source}: {error}"));
    session.collect().unwrap();
    let units = session.inspect(&value, |mut store, value| {
        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
    }).unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn chunk_buffer_matches_jvm_returns_and_failed_store_order() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(printed(&mut session, r#"
          [(let [b (chunk-buffer 2) a (chunk-append b :a) z (chunk-append b :b) c (chunk b)]
             [a z (count b) (count c) (nth c 0) (nth c 1)])
           (let [b (chunk-buffer 0)
                 threw (try (chunk-append b :x) false (catch :default e true))]
             [threw (count b)])
           (let [b (chunk-buffer 1) _ (chunk-append b :first)
                 threw (try (chunk-append b :second) false (catch :default e true))]
             [threw (count b)])
           (let [b (chunk-buffer 2) _ (chunk-append b :first) c (chunk b)
                 threw (try (chunk-append b :second) false (catch :default e true))]
             [threw (count b) (nth c 0)])]"#),
          "[[nil nil 2 2 :a :b] [true 1] [true 2] [true 2 :first]]");
    }
}

#[test]
fn chunked_filter_keeps_the_value_read_before_its_predicate() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // The JVM adapter constructs clojure.lang.ArrayChunk from the same
        // Object[]; array-chunk is Suss's implementation-side factory.
        assert_eq!(printed(&mut session, r#"
          (let [a (object-array [1 2]) events (atom [])
                s (chunk-cons (array-chunk a) nil)
                out (vec (filter (fn [x]
                                  (swap! events conj x)
                                  (when (= x 1) (aset a 0 99))
                                  true) s))]
            [out @events (vec a)])"#), "[[1 2] [1 2] [99 2]]");
    }
}
