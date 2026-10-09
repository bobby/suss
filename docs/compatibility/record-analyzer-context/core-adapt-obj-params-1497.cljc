(core/defn- adapt-obj-params [type [[this & args :as sig] & body]]
  (core/list (vec args)
    (list* 'this-as (vary-meta this assoc :tag type) body)))
