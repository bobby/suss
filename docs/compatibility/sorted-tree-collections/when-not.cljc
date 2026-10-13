(core/defmacro when-not
     "Evaluates test. If logical false, evaluates body in an implicit do."
     [test & body]
     (core/list 'if test nil (cons 'do body)))
