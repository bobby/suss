//! Output, ownership and reentrant writer regressions for the printer buffer.
use suss_cli::{portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session};
use suss_reader::forms::Kind;

#[test]
fn wide_utf16_output_and_empty_chunks_keep_earlier_strings_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        let earlier = session.eval("(pr-str [:earlier])").unwrap();
        let printed = session.eval(r#"(pr-str (vec (repeat 512 "λ🙂\"\\\n")))"#).unwrap();
        session.collect().unwrap();
        let expected = format!("[{}]", vec![r#""λ🙂\"\\\n""#; 512].join(" "));
        for (value, text) in [(&earlier, "[:earlier]"), (&printed, expected.as_str())] {
            assert!(matches!(&bridge.read(&mut session, value, 0..0).unwrap().kind,
                Kind::String(units) if units.iter().copied().eq(text.encode_utf16())));
        }
        for (source, expected) in [("(pr-str)", ""), ("(print-str \"\" \"\")", " "), ("(pr-str [])", "[]")] {
            let value = session.eval(source).unwrap();
            assert!(matches!(&bridge.read(&mut session, &value, 0..0).unwrap().kind,
                Kind::String(units) if units.iter().copied().eq(expected.encode_utf16())), "{source}");
        }
    }
}

#[test]
fn nested_print_method_keeps_outer_write_and_callback_order() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.set_operation_fuel(100_000_000);
        let value = session.eval_with_macros(r#"
          (def events (atom []))
          (defmethod print-method ::nested [x w]
            (swap! events conj :before)
            (.write w "<")
            (.write w (pr-str (:value x)))
            (.write w ">")
            (swap! events conj :after))
          [(pr-str [(with-meta {:value ["λ" 2]} {:type ::nested}) :tail]) @events]
        "#, &mut macros).unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..0).unwrap();
        let Kind::Vector(parts) = &actual.kind else { panic!("printed text and effects") };
        assert!(matches!(&parts[0].kind, Kind::String(units)
            if units.iter().copied().eq("[<[\"λ\" 2]> :tail]".encode_utf16())));
        let Kind::Vector(events) = &parts[1].kind else { panic!("callback order") };
        assert_eq!(events.len(), 2);
        for (event, expected) in events.iter().zip(["before", "after"]) {
            assert!(matches!(&event.kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == expected));
        }
    }
}

#[test]
fn writer_conversion_keeps_the_prefix_captured_before_reentrant_write() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let value = session.eval_with_macros(r#"
          (defmethod print-method ::captured [_ w]
            (.write w "old")
            (.write w (reify Object (toString [_] (.write w "discarded") "x")))
            (.write w "!"))
          (pr-str (with-meta {} {:type ::captured}))
        "#, &mut macros).unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert!(matches!(&bridge.read(&mut session, &value, 0..0).unwrap().kind,
            Kind::String(units) if units.iter().copied().eq("oldx!".encode_utf16())));
    }
}

#[test]
fn long_quoted_strings_preserve_escapes_and_unpaired_utf16_after_gc() {
    use suss_reader::forms::Form;
    let pattern = [97, 34, 92, 10, 0x03bb, 0xd800, 120, 0xdc00];
    let escaped = [97, 92, 34, 92, 92, 92, 110, 0x03bb, 0xd800, 120, 0xdc00];
    let mut expected = vec![34];
    expected.extend(escaped.repeat(4096));
    expected.push(34);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        let input = bridge.quote(&mut session, Form {
            span: 0..0, metadata: vec![], kind: Kind::String(pattern.repeat(4096)),
        }).unwrap();
        let printer = session.eval("pr-str").unwrap();
        let value = session.invoke(&printer, &[&input]).unwrap();
        session.collect().unwrap();
        assert!(matches!(&bridge.read(&mut session, &value, 0..0).unwrap().kind,
            Kind::String(units) if units == &expected));
    }
}
