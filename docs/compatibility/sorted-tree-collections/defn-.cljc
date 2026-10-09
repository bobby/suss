(core/defmacro defn-
     "same as defn, yielding non-public def"
     [name & decls]
     (list* `defn (with-meta name (assoc (meta name) :private true)) decls))
