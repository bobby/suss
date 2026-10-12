// Copyright (c) Rich Hickey. All rights reserved.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php).
// By using this software in any fashion, you agree to be bound by its terms.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0
// Adapted from pinned cljs/core.cljc build-positional-factory (1760–1769)
// and build-map-factory (1960–1970). Complete originals and hashes remain in
// docs/compatibility/record-macro-foundations/provenance.json.
// These functions execute as compiled Suss in the Macro Store. They generate
// ordinary source definitions; no Rust form evaluator constructs the factories.
pub(crate) const SOURCE: &str = r#"
(ns suss.internal.record-factories)

(def ^:private build-positional-factory
  (fn [rsym rname fields]
    (let [fn-name (with-meta (symbol (str '-> rsym))
                    (assoc (meta rsym) :factory :positional))
          docstring (str "Positional factory function for " rname ".")
          field-values (if (-> rsym meta :internal-ctor)
                         (conj fields nil nil nil)
                         fields)]
      `(defn ~fn-name ~docstring [~@fields]
         (new ~rname ~@field-values)))))

(def ^:private build-map-factory
  (fn [rsym rname fields]
    (let [fn-name (with-meta (symbol (str 'map-> rsym))
                    (assoc (meta rsym) :factory :map))
          docstring (str "Factory function for " rname ", taking a map of keywords to field values.")
          ms (gensym)
          ks (map keyword fields)
          getters (map (fn [k] `(~k ~ms)) ks)]
      `(defn ~fn-name ~docstring [~ms]
         (let [raw-extmap# (dissoc ~ms ~@ks)
               extmap# (if (record? ~ms) (into {} raw-extmap#) raw-extmap#)]
           (new ~rname ~@getters nil (not-empty extmap#) nil))))))
"#;
