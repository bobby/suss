(core/defn- type-hint-first-arg
  [type-sym argv]
  (assoc argv 0 (vary-meta (argv 0) assoc :tag type-sym)))
