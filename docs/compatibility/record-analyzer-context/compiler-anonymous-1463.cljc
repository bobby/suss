(defmethod emit* :defrecord
  [{:keys [t fields pmasks body protocols]}]
  (let [fields (concat (map munge fields) '[__meta __extmap __hash])]
    (emitln "")
    (emitln "/**")
    (emitln "* @constructor")
    (doseq [protocol protocols]
      (emitln " * @implements {" (munge (str protocol)) "}"))
    (emitln "*/")
    (emitln (munge t) " = (function (" (comma-sep fields) "){")
    (doseq [fld fields]
      (emitln "this." fld " = " fld ";"))
    (doseq [[pno pmask] pmasks]
      (emitln "this.cljs$lang$protocol_mask$partition" pno "$ = " pmask ";"))
    (emitln "});")
    (emit body)))
