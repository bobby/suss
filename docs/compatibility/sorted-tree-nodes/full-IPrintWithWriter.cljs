(extend-protocol IPrintWithWriter
  LazySeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  TransformerIterator
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  IndexedSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  RSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  PersistentQueue
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "#queue [" " " "]" opts (seq coll)))

  PersistentQueueSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  PersistentTreeMapSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  NodeSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  ArrayNodeSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  List
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  Cons
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  EmptyList
  (-pr-writer [coll writer opts] (-write writer "()"))

  PersistentVector
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "[" " " "]" opts coll))

  ChunkedCons
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  ChunkedSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  Subvec
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "[" " " "]" opts coll))

  BlackNode
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "[" " " "]" opts coll))

  RedNode
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "[" " " "]" opts coll))

  MapEntry
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "[" " " "]" opts coll))

  KeySeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  ValSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  PersistentArrayMapSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  PersistentArrayMap
  (-pr-writer [coll writer opts]
    (print-map coll pr-writer writer opts))

  PersistentHashMap
  (-pr-writer [coll writer opts]
    (print-map coll pr-writer writer opts))

  PersistentTreeMap
  (-pr-writer [coll writer opts]
    (print-map coll pr-writer writer opts))

  PersistentHashSet
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "#{" " " "}" opts coll))

  PersistentTreeSet
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "#{" " " "}" opts coll))

  Range
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  IntegerRange
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  Cycle
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  Repeat
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  Iterate
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  ES6IteratorSeq
  (-pr-writer [coll writer opts] (pr-sequential-writer writer pr-writer "(" " " ")" opts coll))

  Atom
  (-pr-writer [a writer opts]
    (-write writer "#object[cljs.core.Atom ")
    (pr-writer {:val (.-state a)} writer opts)
    (-write writer "]"))

  Volatile
  (-pr-writer [a writer opts]
    (-write writer "#object[cljs.core.Volatile ")
    (pr-writer {:val (.-state a)} writer opts)
    (-write writer "]"))

  Var
  (-pr-writer [a writer opts]
    (-write writer "#'")
    (pr-writer (.-sym a) writer opts)))
