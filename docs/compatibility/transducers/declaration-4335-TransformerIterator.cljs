(deftype TransformerIterator [^:mutable buffer ^:mutable _next ^:mutable completed ^:mutable xf sourceIter multi]
  Object
  (step [this]
    (if-not (identical? _next NONE)
      true
      (loop []
        (if (identical? _next NONE)
          (if ^boolean (.isEmpty buffer)
            (if ^boolean completed
              false
              (if ^boolean (.hasNext sourceIter)
                (let [iter (if ^boolean multi
                             (apply xf (cons nil (.next sourceIter)))
                             (xf nil (.next sourceIter)))]
                  (when (reduced? iter)
                    (xf nil)
                    (set! completed true))
                  (recur))
                (do
                  (xf nil)
                  (set! completed true)
                  (recur))))
            (do
              (set! _next (.remove buffer))
              (recur)))
          true))))
  (hasNext [this]
    (.step this))
  (next [this]
    (if ^boolean (.hasNext this)
      (let [ret _next]
        (set! _next NONE)
        ret)
      (throw (js/Error. "No such element"))))
  (remove [_]
    (js/Error. "Unsupported operation")))
