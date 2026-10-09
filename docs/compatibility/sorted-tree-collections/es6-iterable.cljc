(core/defmacro es6-iterable [ty]
  `(unchecked-set (.-prototype ~ty) cljs.core/ITER_SYMBOL
     (fn []
       (this-as this#
         (cljs.core/es6-iterator this#)))))
