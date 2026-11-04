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

// defmulti: Clojure's, with check-valid-options inlined. Its errors are plain
// exceptions, as Clojure's Exception and IllegalArgumentException are. The
// default hierarchy is the global hierarchy's reference, as #'global-hierarchy
// is Clojure's. Clojure defines the multimethod only when its var has no root
// or holds something else; `defonce` binds an unbound var first, so the
// check is the same.
pub(crate) const DEFMULTI: &str = r##"
(defmacro defmulti
  "Creates a new multimethod with the associated dispatch function.
  The docstring and attr-map are optional.

  Options are key-value pairs and may be one of:

  :default

  The default dispatch value, defaults to :default

  :hierarchy

  The value used for hierarchical dispatch (e.g. ::square is-a ::shape)

  Hierarchies are type-like relationships that do not depend upon type
  inheritance. By default Clojure's multimethods dispatch off of a
  global hierarchy map.  However, a hierarchy relationship can be
  created with the derive function used to augment the root ancestor
  created with make-hierarchy.

  Multimethods expect the value of the hierarchy option to be supplied as
  a reference type e.g. a var (i.e. via the Var-quote dispatch macro #'
  or the var special form)."
  [mm-name & options]
  (let [docstring   (if (string? (first options))
                      (first options)
                      nil)
        options     (if (string? (first options))
                      (next options)
                      options)
        m           (if (map? (first options))
                      (first options)
                      {})
        options     (if (map? (first options))
                      (next options)
                      options)
        dispatch-fn (first options)
        options     (next options)
        m           (if docstring
                      (assoc m :doc docstring)
                      m)
        m           (if (meta mm-name)
                      (conj (meta mm-name) m)
                      m)
        mm-name (with-meta mm-name m)]
    (if (= (count options) 1)
      (throw (suss.bootstrap/error "The syntax for defmulti has changed. Example: (defmulti name dispatch-fn :default dispatch-value)"))
      nil)
    (let [options   (apply hash-map options)
          default   (get options :default :default)
          hierarchy (get options :hierarchy `(get-global-hierarchy))]
      (if (seq (disj (apply hash-set (keys options)) :default :hierarchy))
        (throw (suss.bootstrap/error "Only these options are valid: :default, :hierarchy"))
        nil)
      `(do
         (defonce ~(with-meta mm-name nil) nil)
         (when-not (instance? MultiFn ~mm-name)
           (def ~mm-name
                (new MultiFn ~(name mm-name) ~dispatch-fn ~default ~hierarchy
                     (hash-map) (hash-map) (hash-map) nil)))))))
"##;

// defmethod: Clojure's, calling the protocol method where Clojure calls
// MultiFn.addMethod.
pub(crate) const DEFMETHOD: &str = r#"
(defmacro defmethod
  "Creates and installs a new method of multimethod associated with dispatch-value. "
  [multifn dispatch-val & fn-tail]
  `(-add-method ~multifn ~dispatch-val (fn ~@fn-tail)))
"#;
