(core/defmacro let
  "binding => binding-form init-expr
  binding-form => name, or destructuring-form
  destructuring-form => map-destructure-form, or seq-destructure-form

  Evaluates the exprs in a lexical context in which the symbols in
  the binding-forms are bound to their respective init-exprs or parts
  therein.

  See https://clojure.org/reference/special_forms#binding-forms for
  more information about destructuring."
  [bindings & body]
  (assert-args let
     (vector? bindings) "a vector for its binding"
     (even? (count bindings)) "an even number of forms in binding vector")
  `(let* ~(destructure bindings) ~@body))
