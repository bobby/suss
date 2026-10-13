(extend-protocol IComparable
  Symbol
  (-compare [x y]
    (if (symbol? y)
      (compare-symbols x y)
      (throw (js/Error. (str_ "Cannot compare " x " to " y)))))

  Keyword
  (-compare [x y]
    (if (keyword? y)
      (compare-keywords x y)
      (throw (js/Error. (str_ "Cannot compare " x " to " y)))))

  Subvec
  (-compare [x y]
    (if (vector? y)
      (compare-indexed x y)
      (throw (js/Error. (str_ "Cannot compare " x " to " y)))))

  PersistentVector
  (-compare [x y]
    (if (vector? y)
      (compare-indexed x y)
      (throw (js/Error. (str_ "Cannot compare " x " to " y)))))

  MapEntry
  (-compare [x y]
    (if (vector? y)
      (compare-indexed x y)
      (throw (js/Error. (str_ "Cannot compare " x " to " y)))))

  BlackNode
  (-compare [x y]
    (if (vector? y)
      (compare-indexed x y)
      (throw (js/Error. (str_ "Cannot compare " x " to " y)))))

  RedNode
  (-compare [x y]
    (if (vector? y)
      (compare-indexed x y)
      (throw (js/Error. (str_ "Cannot compare " x " to " y))))))
