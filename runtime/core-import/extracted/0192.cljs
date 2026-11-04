;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype MapEntry [key val ^:mutable __hash]
  Object
  (indexOf [coll x]
    (-indexOf coll x 0))
  (indexOf [coll x start]
    (-indexOf coll x start))
  (lastIndexOf [coll x]
    (-lastIndexOf coll x (count coll)))
  (lastIndexOf [coll x start]
    (-lastIndexOf coll x start))

  IMapEntry
  (-key [node] key)
  (-val [node] val)

  IHash
  (-hash [coll] (caching-hash coll hash-ordered-coll __hash))

  IEquiv
  (-equiv [coll other] (equiv-sequential coll other))

  IMeta
  (-meta [node] nil)

  IWithMeta
  (-with-meta [node meta]
    (with-meta [key val] meta))

  IStack
  (-peek [node] val)

  (-pop [node] [key])

  ICollection
  (-conj [node o] [key val o])

  IEmptyableCollection
  (-empty [node] nil)

  ISequential
  ISeqable
  (-seq [node] (IndexedSeq. #js [key val] 0 nil))

  IReversible
  (-rseq [node] (IndexedSeq. #js [val key] 0 nil))

  ICounted
  (-count [node] 2)

  IIndexed
  (-nth [node n]
    (case n
      0 key
      1 val
      (throw (js/Error. "Index out of bounds"))))

  (-nth [node n not-found]
    (case n
      0 key
      1 val
      not-found))

  ILookup
  (-lookup [node k] (-nth node k nil))
  (-lookup [node k not-found] (-nth node k not-found))

  IAssociative
  (-assoc [node k v]
    (assoc [key val] k v))
  (-contains-key? [node k]
    (case k
      0 true
      1 true
      false))

  IFind
  (-find [node k]
    (case k
      0 (MapEntry. 0 key nil)
      1 (MapEntry. 1 val nil)
      nil))

  IVector
  (-assoc-n [node n v]
    (-assoc-n [key val] n v))

  IReduce
  (-reduce [node f]
    (ci-reduce node f))

  (-reduce [node f start]
    (ci-reduce node f start))

  IFn
  (-invoke [node k]
    (-nth node k))

  (-invoke [node k not-found]
    (-nth node k not-found)))
