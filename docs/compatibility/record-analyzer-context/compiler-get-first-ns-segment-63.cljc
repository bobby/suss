(defn get-first-ns-segment
  "Gets the part up to the first `.` of a namespace.
   Returns the empty string for nil.
   Returns the entire string if no `.` in namespace"
  [ns]
  (let [ns (str ns)
        idx (.indexOf ns ".")]
    (if (== -1 idx)
      ns
      (subs ns 0 idx))))
