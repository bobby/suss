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

;; Static private construction adaptation; general syntax quote is unchanged.
(ns suss.internal.record-factories)
(def ^:private factory-join3 (fn [a b c] (suss.bootstrap/concat-string (suss.bootstrap/concat-string a b) c)))
(def ^:private template-sequence (fn* [coll] (if (suss.core/seq? coll) coll (suss.core/or (suss.core/seq coll) ()))))
(def ^:private build-positional-factory (fn [rsym rname fields] (let [fn-name (with-meta (symbol (suss.bootstrap/concat-string "->" (str rsym))) (assoc (meta rsym) :factory :positional)) docstring (factory-join3 "\u0050\u006f\u0073\u0069\u0074\u0069\u006f\u006e\u0061\u006c\u0020\u0066\u0061\u0063\u0074\u006f\u0072\u0079\u0020\u0066\u0075\u006e\u0063\u0074\u0069\u006f\u006e\u0020\u0066\u006f\u0072\u0020" (str rname) "\u002e") field-values (if (-> rsym meta :internal-ctor) (conj fields nil nil nil) fields)] (suss.core/Cons. nil (quote cljs.core/defn) (suss.core/Cons. nil fn-name (suss.core/Cons. nil docstring (suss.core/Cons. nil (clojure.core/vec (suss.internal.record-factories/template-sequence (clojure.core/seq (clojure.core/concat fields)))) (suss.core/Cons. nil (suss.internal.record-factories/template-sequence (clojure.core/seq (clojure.core/concat (clojure.core/list (quote new)) (clojure.core/list rname) field-values))) (quote ()) nil) nil) nil) nil) nil))))
(def ^:private template-constant-0 (suss.core/Cons. nil (quote raw-extmap__3__auto__) (quote ()) nil))
(def ^:private template-constant-1 (suss.core/Cons. nil (quote raw-extmap__3__auto__) (quote ()) nil))
(def ^:private template-constant-2 (suss.core/Cons. nil (quote cljs.core/not-empty) (suss.core/Cons. nil (quote extmap__5__auto__) (quote ()) nil) nil))
(def ^:private build-map-factory (fn [rsym rname fields] (let [fn-name (with-meta (symbol (suss.bootstrap/concat-string "map->" (str rsym))) (assoc (meta rsym) :factory :map)) docstring (factory-join3 "\u0046\u0061\u0063\u0074\u006f\u0072\u0079\u0020\u0066\u0075\u006e\u0063\u0074\u0069\u006f\u006e\u0020\u0066\u006f\u0072\u0020" (str rname) "\u002c\u0020\u0074\u0061\u006b\u0069\u006e\u0067\u0020\u0061\u0020\u006d\u0061\u0070\u0020\u006f\u0066\u0020\u006b\u0065\u0079\u0077\u006f\u0072\u0064\u0073\u0020\u0074\u006f\u0020\u0066\u0069\u0065\u006c\u0064\u0020\u0076\u0061\u006c\u0075\u0065\u0073\u002e") ms (gensym) ks (map keyword fields) getters (map (fn [k] (suss.core/Cons. nil k (suss.core/Cons. nil ms (quote ()) nil) nil)) ks)] (suss.core/Cons. nil (quote cljs.core/defn) (suss.core/Cons. nil fn-name (suss.core/Cons. nil docstring (suss.core/Cons. nil [ms] (suss.core/Cons. nil (suss.core/Cons. nil (quote cljs.core/let) (suss.core/Cons. nil [(quote raw-extmap__3__auto__) (suss.internal.record-factories/template-sequence (clojure.core/seq (clojure.core/concat (clojure.core/list (quote cljs.core/dissoc)) (clojure.core/list ms) ks))) (quote extmap__5__auto__) (suss.core/Cons. nil (quote if) (suss.core/Cons. nil (suss.core/Cons. nil (quote cljs.core/record?) (suss.core/Cons. nil ms (quote ()) nil) nil) (suss.core/Cons. nil (suss.core/Cons. nil (quote cljs.core/into) (suss.core/Cons. nil (clojure.core/apply clojure.core/array-map (quote ())) suss.internal.record-factories/template-constant-0 nil) nil) suss.internal.record-factories/template-constant-1 nil) nil) nil)] (suss.core/Cons. nil (suss.internal.record-factories/template-sequence (clojure.core/seq (clojure.core/concat (clojure.core/list (quote new)) (clojure.core/list rname) getters (clojure.core/list nil) (clojure.core/list suss.internal.record-factories/template-constant-2) (clojure.core/list nil)))) (quote ()) nil) nil) nil) (quote ()) nil) nil) nil) nil) nil))))

"#;
