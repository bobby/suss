(core/defmethod extend-prefix :instance
  [tsym sym] `(.. ~tsym ~(to-property sym)))
