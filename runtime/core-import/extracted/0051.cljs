;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype EmptyList [meta]
  Object
  (toString [coll] "()")
  (equiv [this other]
    (-equiv this other))
  (indexOf [coll x]
    (-indexOf coll x 0))
  (indexOf [coll x start]
    (-indexOf coll x start))
  (lastIndexOf [coll x]
    (-lastIndexOf coll x (count coll)))
  (lastIndexOf [coll x start]
    (-lastIndexOf coll x start))

  IList

  ICloneable
  (-clone [_] (EmptyList. meta))

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (EmptyList. new-meta)))

  IMeta
  (-meta [coll] meta)

  ISeq
  (-first [coll] nil)
  (-rest [coll] ())

  INext
  (-next [coll] nil)

  IStack
  (-peek [coll] nil)
  (-pop [coll] (throw (js/Error. "Can't pop empty list")))

  ICollection
  (-conj [coll o] (List. meta o nil 1 nil))

  IEmptyableCollection
  (-empty [coll] coll)

  ISequential
  IEquiv
  (-equiv [coll other]
    (if (or (list? other)
            (sequential? other))
      (nil? (seq other))
      false))

  IHash
  (-hash [coll] empty-ordered-hash)

  ISeqable
  (-seq [coll] nil)

  ICounted
  (-count [coll] 0)

  IReduce
  (-reduce [coll f] (seq-reduce f coll))
  (-reduce [coll f start] (seq-reduce f start coll)))
