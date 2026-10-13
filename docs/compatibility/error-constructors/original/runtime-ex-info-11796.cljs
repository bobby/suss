(defn ex-info
  "Create an instance of ExceptionInfo, an Error type that carries a
  map of additional data."
  ([msg data] (ex-info msg data nil))
  ([msg data cause]
    (ExceptionInfo. msg data cause)))
