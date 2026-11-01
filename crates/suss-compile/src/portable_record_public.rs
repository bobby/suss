// Copyright (c) Rich Hickey. All rights reserved.
// SPDX-License-Identifier: EPL-1.0
// Complete pinned upstream sources/notices remain in
// docs/compatibility/record-macro-foundations. These helpers execute compiled
// Suss; no Rust form evaluator builds record expansions.
pub(crate) const SOURCE: &str = r####"
;; Copyright (c) Rich Hickey. All rights reserved.
;; The use and distribution terms for this software are covered by the
;; Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php).
;; By using this software in any fashion, you agree to be bound by its terms.
;; You must not remove this notice, or any other, from this software.
;; SPDX-License-Identifier: EPL-1.0
;; Integrated compiled adaptation of complete core.cljc defrecord / validate-fields.
;; Complete originals retained in docs/compatibility/record-macro-foundations.
;; Public CLI and independent post-GC ABI decoding checks pass.
;; Full record customization and M4 acceptance remain unproven.
;; Each record registers its factories for read-string's record literals (#300).
;; Static private construction adaptation; general syntax quote is unchanged.
(ns suss.internal.record-public)
(def ^:private template-sequence (fn* [coll] (if (suss.core/seq? coll) coll (suss.core/or (suss.core/seq coll) ()))))
(def ^:private template-constant-0 [])
(def ^:private template-constant-1 [])
(def ^:private template-constant-2 (suss.core/Cons. nil true (quote ()) nil))
(def ^:private template-constant-3 [(quote this__3__auto__)])
(def ^:private template-constant-4 [(quote this__3__auto__) (quote writer__5__auto__)])
(def emit-public-record (fn [&env rsym fields impls] (when-not (vector? fields) (throw (suss.bootstrap/error (str "\u0064\u0065\u0066\u0072\u0065\u0063\u006f\u0072\u0064\u0020" rsym "\u002c\u0020\u006e\u006f\u0020\u0066\u0069\u0065\u006c\u0064\u0073\u0020\u0076\u0065\u0063\u0074\u006f\u0072\u0020\u0067\u0069\u0076\u0065\u006e\u002e")))) (let [rsym (vary-meta rsym assoc :internal-ctor true) r (vary-meta (symbol (str (:name (:ns &env))) (name rsym)) assoc :internal-ctor true)] (suss.core/Cons. nil (quote cljs.core/let) (suss.core/Cons. nil suss.internal.record-public/template-constant-0 (suss.core/Cons. nil (suss.internal.record-emitter/emit-defrecord &env rsym r fields impls) (suss.core/Cons. nil (suss.core/Cons. nil (quote set!) (suss.core/Cons. nil (suss.core/Cons. nil (quote .-getBasis) (suss.core/Cons. nil r (quote ()) nil) nil) (suss.core/Cons. nil (suss.core/Cons. nil (quote cljs.core/fn) (suss.core/Cons. nil suss.internal.record-public/template-constant-1 (suss.core/Cons. nil (suss.core/Cons. nil (quote quote) (suss.core/Cons. nil (clojure.core/vec fields) (quote ()) nil) nil) (quote ()) nil) nil) nil) (quote ()) nil) nil) nil) (suss.core/Cons. nil (suss.core/Cons. nil (quote set!) (suss.core/Cons. nil (suss.core/Cons. nil (quote .-cljs$lang$type) (suss.core/Cons. nil r (quote ()) nil) nil) suss.internal.record-public/template-constant-2 nil) nil) (suss.core/Cons. nil (suss.core/Cons. nil (quote set!) (suss.core/Cons. nil (suss.core/Cons. nil (quote .-cljs$lang$ctorPrSeq) (suss.core/Cons. nil r (quote ()) nil) nil) (suss.core/Cons. nil (suss.core/Cons. nil (quote cljs.core/fn) (suss.core/Cons. nil suss.internal.record-public/template-constant-3 (suss.core/Cons. nil (suss.core/Cons. nil (quote cljs.core/list) (suss.core/Cons. nil (str r) (quote ()) nil) nil) (quote ()) nil) nil) nil) (quote ()) nil) nil) nil) (suss.core/Cons. nil (suss.core/Cons. nil (quote set!) (suss.core/Cons. nil (suss.core/Cons. nil (quote .-cljs$lang$ctorPrWriter) (suss.core/Cons. nil r (quote ()) nil) nil) (suss.core/Cons. nil (suss.core/Cons. nil (quote cljs.core/fn) (suss.core/Cons. nil suss.internal.record-public/template-constant-4 (suss.core/Cons. nil (suss.core/Cons. nil (quote cljs.core/-write) (suss.core/Cons. nil (quote writer__5__auto__) (suss.core/Cons. nil (str r) (quote ()) nil) nil) nil) (quote ()) nil) nil) nil) (quote ()) nil) nil) nil) (suss.core/Cons. nil (suss.internal.record-factories/build-positional-factory rsym r fields) (suss.core/Cons. nil (suss.internal.record-factories/build-map-factory rsym r fields) (suss.core/Cons. nil (suss.core/Cons. nil (quote suss.core/register-record-reader!) (suss.core/Cons. nil (str (namespace r) "\u002e" (name r)) (suss.core/Cons. nil (symbol (str "\u002d\u003e" (name rsym))) (suss.core/Cons. nil (count fields) (suss.core/Cons. nil (symbol (str "\u006d\u0061\u0070\u002d\u003e" (name rsym))) (quote ()) nil) nil) nil) nil) nil) (suss.core/Cons. nil r (quote ()) nil) nil) nil) nil) nil) nil) nil) nil) nil) nil) nil))))

"####;

pub(crate) const MACRO_SOURCE: &str = r####"(defmacro ^{:suss/compiler-catalog :record-bindings} defrecord [rsym fields & impls]
  (suss.internal.record-public/emit-public-record &env rsym fields impls))
"####;
