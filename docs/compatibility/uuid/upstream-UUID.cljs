(deftype UUID [uuid ^:mutable __hash]
  IUUID

  Object
  (toString [_] uuid)
  (equiv [this other]
    (-equiv this other))

  IEquiv
  (-equiv [_ other]
    (and (implements? IUUID other) (identical? uuid (.-uuid other))))

  IPrintWithWriter
  (-pr-writer [_ writer _]
    (-write writer (str_ "#uuid \"" uuid "\"")))

  IHash
  (-hash [this]
    (when (nil? __hash)
      (set! __hash (hash uuid)))
    __hash)

  IComparable
  (-compare [this other]
    (if (instance? UUID other)
      (garray/defaultCompare uuid (.-uuid other))
      (throw (js/Error. (str_ "Cannot compare " this " to " other))))))
