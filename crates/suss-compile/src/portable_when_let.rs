// Copyright (c) Rich Hickey. All rights reserved.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php).
// By using this software in any fashion, you agree to be bound by its terms.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0
// Adapted from cljs/core.cljc:412-424 and cljs/support.cljc:11-18 at
// c4295f303100bbf5afac449242d30bca1126f1a1. See
// docs/compatibility/when-let/provenance.json and runtime/core-import/epl-v10.html.
// The ordered assert-args checks are expanded into ordinary compiled source;
// vector invocation remains live. No native macro evaluator is involved.
pub(crate) const SOURCE: &str = r#"
(defmacro when-let
     "bindings => binding-form test

     When test is true, evaluates body with binding-form bound to the value of test"
     [bindings & body]
     (if (vector? bindings) nil
       (throw (ex-info "when-let requires a vector for its binding"
                       {:clojure.error/phase :macro-syntax-check})))
     (if (= 2 (count bindings)) nil
       (throw (ex-info "when-let requires exactly 2 forms in binding vector"
                       {:clojure.error/phase :macro-syntax-check})))
     (let [form (bindings 0) tst (bindings 1)]
       `(let [temp# ~tst]
          (when temp#
            (let [~form temp#]
              ~@body)))))
"#;
