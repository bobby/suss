(deftype SeqIter [^:mutable _seq ^:mutable _next]
  Object
  (hasNext [_]
    (if (identical? _seq INIT)
      (do
        (set! _seq START)
        (set! _next (seq _next)))
      (if (identical? _seq _next)
        (set! _next (next _seq))))
    (not (nil? _next)))
  (next [this]
    (if-not ^boolean (.hasNext this)
      (throw (js/Error. "No such element"))
      (do
        (set! _seq _next)
        (first _next))))
  (remove [_] (js/Error. "Unsupported operation")))
