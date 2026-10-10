(core/defmethod extend-prefix :default
  [tsym sym]
  (with-meta `(.. ~tsym ~'-prototype ~(to-property sym)) {:extend-type true}))
