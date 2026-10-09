(core/defn- adapt-ifn-params [type [[this & args :as sig] & body]]
  (core/let [self-sym (with-meta 'self__ {:tag type})]
    `(~(vec (cons self-sym args))
       (this-as ~self-sym
         (let [~this ~self-sym]
           ~@body)))))
