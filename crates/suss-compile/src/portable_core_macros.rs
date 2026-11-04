// Copyright (c) Rich Hickey. All rights reserved.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php).
// By using this software in any fashion, you agree to be bound by its terms.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0
// Adapted from Clojure 1.12's clojure/core.clj (#30).
//
// doseq: as for when-let,
// the assert-args checks are expanded into ordinary compiled source and
// their messages omit Clojure's "in ns:line" suffix. Clojure also walks a
// chunked seq a chunk at a time; that branch is left out. It changes no
// result or effect order, since a chunked seq realizes each chunk when it is
// first seq'd either way, and it copies the body into both branches at every
// binding, doubling the expansion per binding.
pub(crate) const DOSEQ: &str = r#"
(defmacro doseq
  "Repeatedly executes body (presumably for side-effects) with
  bindings and filtering as provided by \"for\".  Does not retain
  the head of the sequence. Returns nil."
  [seq-exprs & body]
  (if (vector? seq-exprs) nil
    (throw (ex-info "doseq requires a vector for its binding"
                    {:clojure.error/phase :macro-syntax-check})))
  (if (even? (count seq-exprs)) nil
    (throw (ex-info "doseq requires an even number of forms in binding vector"
                    {:clojure.error/phase :macro-syntax-check})))
  (let [step (fn step [recform exprs]
               (if-not exprs
                 [true `(do ~@body)]
                 (let [k (first exprs)
                       v (second exprs)]
                   (if (keyword? k)
                     (let [steppair (step recform (nnext exprs))
                           needrec (steppair 0)
                           subform (steppair 1)]
                       (cond
                         (= k :let) [needrec `(let ~v ~subform)]
                         (= k :while) [false `(when ~v
                                                ~subform
                                                ~@(when needrec [recform]))]
                         (= k :when) [false `(if ~v
                                               (do
                                                 ~subform
                                                 ~@(when needrec [recform]))
                                               ~recform)]))
                     (let [seq- (gensym "seq_")
                           recform `(recur (next ~seq-))
                           steppair (step recform (nnext exprs))
                           needrec (steppair 0)
                           subform (steppair 1)]
                       [true
                        `(loop [~seq- (seq ~v)]
                           (when ~seq-
                             (let [~k (first ~seq-)]
                               ~subform
                               ~@(when needrec [recform]))))])))))]
    (nth (step nil (seq seq-exprs)) 1)))
"#;

// ->>: Clojure's, unchanged; -> is a compiler intrinsic.
pub(crate) const THREAD_LAST: &str = r#"
(defmacro ->>
  "Threads the expr through the forms. Inserts x as the
  last item in the first form, making a list of it if it is not a
  list already. If there are more forms, inserts the first form as the
  last item in second form, etc."
  [x & forms]
  (loop [x x, forms forms]
    (if forms
      (let [form (first forms)
            threaded (if (seq? form)
                       (with-meta `(~(first form) ~@(next form) ~x) (meta form))
                       (list form x))]
        (recur threaded (next forms)))
      x)))
"#;
