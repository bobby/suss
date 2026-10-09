(core/defn- validate-fields
  [case name fields]
  (core/when-not (vector? fields)
    (throw
      #?(:clj (AssertionError. (core/str case " " name ", no fields vector given."))
         :cljs (js/Error. (core/str case " " name ", no fields vector given."))))))
