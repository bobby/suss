(core/defmacro identical? [a b]
  (bool-expr (core/list 'js* "(~{} === ~{})" a b)))
