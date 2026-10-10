(core/defn- adapt-proto-params [type [[this & args :as sig] & body]]
  (core/let [this' (vary-meta this assoc :tag type)]
    `(~(vec (cons this' args))
      (this-as ~this'
        ~@body))))
