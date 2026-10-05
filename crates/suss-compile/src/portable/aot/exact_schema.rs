//! Original private exact-integer boundary schema. Capture nominal identities
//! before user initialization, retaining the existing shared ten-type GC ABI.
pub(super) const SOURCE: &str = r#"
(let [signed-class suss.core/WitSigned64
      unsigned-class suss.core/WitUnsigned64
      array suss.core/array
      get suss.core/aget
      valid-word suss.core/wit-valid-word?]
  (fn [operation first second]
    (if (suss.core/identical? operation 0)
      (new signed-class first second)
      (if (suss.core/identical? operation 1)
        (new unsigned-class first second)
        (if (if (suss.core/identical? operation 2) true
                (suss.core/identical? operation 3))
          (if (if (suss.core/identical? operation 2)
                (suss.core/instance? signed-class first)
                (suss.core/instance? unsigned-class first))
            (let [high (.-high first) low (.-low first)]
              (if (if (valid-word high) (valid-word low) false)
                (array high low)
                (throw "WIT exact integer 64-bit result has invalid word storage")))
            (throw "WIT exact integer 64-bit result requires the selected signed/unsigned wrapper"))
          (get first (if (suss.core/identical? operation 4) 0 1)))))))
"#;
